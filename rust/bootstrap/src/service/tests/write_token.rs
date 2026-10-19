//! Write token checks on client put, replica put and repair.
use super::*;
use mailbox_role::{check_token, TokenError};

const DAY: i64 = 86_400;

pub(super) fn today() -> u32 {
    (chrono::Utc::now().timestamp() / DAY) as u32
}

/// A token for `owner`'s mailbox handed to `token_key` on `epoch`.
pub(super) fn token(
    owner: &Identity,
    mailbox: &[u8],
    token_key: &Identity,
    epoch: u32,
) -> SignedRecord {
    let payload = proto::WriteToken {
        mailbox: mailbox.to_vec(),
        token_key: token_key.public_key().to_vec(),
        epoch,
    };
    owner.sign(Domain::WriteToken, payload.encode_to_vec())
}

pub(super) fn envelope(id: u8, mailbox: &[u8], token: SignedRecord) -> proto::Envelope {
    proto::Envelope {
        id: vec![id; 16],
        mailbox: mailbox.to_vec(),
        ciphertext: vec![1],
        write_token: Some(token),
        ..Default::default()
    }
}

pub(super) fn put(service: &Service, from: &Peer, request: mailbox_request::Request) -> Status {
    let request = MailboxRequest {
        request: Some(request),
    };
    mailbox_status(&service.mailbox(from, &mut None, request).unwrap())
}

pub(super) fn mailbox_of(owner: &Identity) -> Vec<u8> {
    dyapp_identity::key_hash(&owner.public_key()).to_vec()
}

#[test]
fn client_put_denies_forged_tokens_with_a_strike_and_expired_without() {
    let (owner, contact, stranger) = (
        Identity::generate(),
        Identity::generate(),
        Identity::generate(),
    );
    let mailbox = mailbox_of(&owner);
    let other = mailbox_of(&stranger);
    let valid = token(&owner, &mailbox, &contact, today());
    let signed = |by: &Identity, envelope: proto::Envelope| {
        mailbox_request::Request::Put(by.sign(Domain::Envelope, envelope.encode_to_vec()))
    };
    let service = service_with(|l| l.strikes_to_ban = 1);
    assert_eq!(
        put(
            &service,
            &peer("p"),
            signed(&contact, envelope(1, &mailbox, valid.clone()))
        ),
        Status::Ok
    );
    assert_eq!(service.held_ids(&mailbox).unwrap(), vec![vec![1; 16]]);
    let expired = token(&owner, &mailbox, &contact, today() - 1);
    assert_eq!(
        put(
            &service,
            &peer("p"),
            signed(&contact, envelope(2, &mailbox, expired))
        ),
        Status::Stale
    );
    assert!(!service.reputation.banned("p"));
    let mut bad_signature = valid.clone();
    bad_signature.signature[0] ^= 1;
    let forged = [
        // Bad token signature.
        signed(&contact, envelope(3, &mailbox, bad_signature)),
        // Signed by a stranger, not the mailbox's device key.
        signed(
            &contact,
            envelope(3, &mailbox, token(&stranger, &mailbox, &contact, today())),
        ),
        // A token for another mailbox.
        signed(&contact, envelope(3, &other, valid.clone())),
        signed(
            &contact,
            envelope(3, &other, token(&owner, &other, &contact, today())),
        ),
        // The envelope is not signed by the token key.
        signed(&stranger, envelope(3, &mailbox, valid)),
    ];
    for (i, request) in forged.into_iter().enumerate() {
        let service = service_with(|l| l.strikes_to_ban = 1);
        assert_eq!(put(&service, &peer("p"), request), Status::Denied, "{i}");
        assert!(service.reputation.banned("p"), "{i}");
        assert!(service.held_ids(&mailbox).unwrap().is_empty());
    }
}

#[test]
fn replica_put_drops_bad_tokens_without_a_strike() {
    let (owner, contact, stranger) = (
        Identity::generate(),
        Identity::generate(),
        Identity::generate(),
    );
    let mailbox = mailbox_of(&owner);
    let service = service_with(|l| l.strikes_to_ban = 1);
    let good = token(&owner, &mailbox, &contact, today());
    let batch = proto::Envelopes {
        envelopes: [
            (1, &contact, token(&owner, &mailbox, &contact, today() - 30)),
            (2, &stranger, good.clone()),
            (3, &contact, token(&stranger, &mailbox, &contact, today())),
            (4, &contact, good),
        ]
        .into_iter()
        .map(|(id, by, token)| {
            by.sign(
                Domain::Envelope,
                envelope(id, &mailbox, token).encode_to_vec(),
            )
        })
        .collect(),
        ..Default::default()
    };
    let replica = replica_peer("replica");
    assert_eq!(
        put(
            &service,
            &replica,
            mailbox_request::Request::ReplicaPut(batch)
        ),
        Status::Ok
    );
    assert_eq!(service.held_ids(&mailbox).unwrap(), vec![vec![4; 16]]);
    assert!(!service.reputation.banned(&replica.id));
}

#[test]
fn token_windows_follow_the_skew_and_the_ttl() {
    let (owner, contact) = (Identity::generate(), Identity::generate());
    let mailbox = mailbox_of(&owner);
    let epoch = 20_000;
    let record = contact.sign(
        Domain::Envelope,
        envelope(1, &mailbox, token(&owner, &mailbox, &contact, epoch)).encode_to_vec(),
    );
    let check = |now: i64, late: i64| {
        let envelope = proto::Envelope::decode(record.payload.as_slice()).unwrap();
        check_token(&record, &envelope, now, late)
    };
    let (start, end) = (i64::from(epoch) * DAY, i64::from(epoch + 1) * DAY);
    let key = Ok(Some(contact.public_key().to_vec()));
    // Client: the day with one hour of skew either side.
    assert_eq!(check(start - 3600, 3600), key);
    assert_eq!(check(start - 3601, 3600), Err(TokenError::Expired));
    assert_eq!(check(end + 3599, 3600), key);
    assert_eq!(check(end + 3600, 3600), Err(TokenError::Expired));
    // Replicas with message_ttl_hours = 72: until the day's end + 73 h.
    let late = 73 * 3600;
    assert_eq!(check(end + 48 * 3600, late), key);
    assert_eq!(check(end + 74 * 3600, late), Err(TokenError::Expired));
    let plain = Identity::generate().sign(Domain::Envelope, vec![]);
    assert_eq!(
        check_token(&plain, &proto::Envelope::default(), 0, 0),
        Ok(None)
    );
}

/// A tokenless envelope with a 4-byte `id` repeated to 16 bytes.
fn plain(id: u32, mailbox: &[u8], class: proto::EnvelopeClass, len: usize) -> proto::Envelope {
    proto::Envelope {
        id: id.to_be_bytes().repeat(4),
        mailbox: mailbox.to_vec(),
        ciphertext: vec![0; len],
        class: class.into(),
        ..Default::default()
    }
}

/// Bytes of tokenless envelopes a mailbox holds.
fn tokenless_bytes(service: &Service, mailbox: &[u8]) -> usize {
    service
        .held(mailbox)
        .unwrap()
        .values()
        .filter(|(r, _)| {
            proto::Envelope::decode(r.payload.as_slice())
                .unwrap()
                .write_token
                .is_none()
        })
        .map(|(r, _)| r.encoded_len())
        .sum()
}

fn pooled_service(change: impl FnOnce(&mut crate::config::Limits)) -> Service {
    service_with(|l| {
        l.require_write_tokens = true;
        l.mailbox_pool_kb = 101;
        l.sender_puts_per_second = 100_000;
        change(l);
    })
}

#[test]
fn a_tokenless_flood_stays_in_the_pool_and_spares_other_groups_and_tokens() {
    use mailbox_role::{Group, Source};
    use proto::EnvelopeClass::Message;
    let (owner, contact, sender) = (
        Identity::generate(),
        Identity::generate(),
        Identity::generate(),
    );
    let mailbox = mailbox_of(&owner);
    let service = pooled_service(|_| {});
    let put = |envelope: proto::Envelope, by: &Identity, group| {
        let record = by.sign(Domain::Envelope, envelope.encode_to_vec());
        let response = service
            .put(
                &record,
                true,
                Source::Client,
                MAX_ENVELOPE_BYTES,
                "p",
                group,
            )
            .unwrap();
        mailbox_status(&response)
    };
    let first = plain(0, &mailbox, Message, 10_000);
    assert_eq!(put(first.clone(), &sender, Group::Ip("b")), Status::Ok);
    for id in 1..=20 {
        let flood = plain(id, &mailbox, Message, 10_000);
        assert_eq!(put(flood, &sender, Group::Ip("a")), Status::Ok);
    }
    assert!(tokenless_bytes(&service, &mailbox) <= 101 * 1024);
    let mut tokened = envelope(9, &mailbox, token(&owner, &mailbox, &contact, today()));
    tokened.ciphertext = vec![0; 50_000];
    assert_eq!(put(tokened.clone(), &contact, Group::Ip("a")), Status::Ok);
    let held = service.held_ids(&mailbox).unwrap();
    assert!(held.contains(&first.id) && held.contains(&tokened.id));
    assert!(tokenless_bytes(&service, &mailbox) <= 101 * 1024);
}

#[test]
fn sprayed_replica_labels_evict_only_that_replicas_envelopes() {
    use mailbox_role::{Group, Source};
    use proto::EnvelopeClass::Message;
    let (owner, sender) = (Identity::generate(), Identity::generate());
    let mailbox = mailbox_of(&owner);
    let service = pooled_service(|_| {});
    let sign = |envelope: &proto::Envelope| sender.sign(Domain::Envelope, envelope.encode_to_vec());
    let local = plain(0, &mailbox, Message, 1_000);
    let response = service.store_mailbox(&sign(&local), "node", &[7; 8]);
    assert_eq!(mailbox_status(&response.unwrap()), Status::Ok);
    let client = plain(1, &mailbox, Message, 1_000);
    let response = service.put(
        &sign(&client),
        true,
        Source::Client,
        MAX_ENVELOPE_BYTES,
        "client",
        Group::Ip("a"),
    );
    assert_eq!(mailbox_status(&response.unwrap()), Status::Ok);
    let spray: Vec<_> = (2..1002)
        .map(|id| plain(id, &mailbox, Message, 200))
        .collect();
    let batch = proto::Envelopes {
        envelopes: spray.iter().map(sign).collect(),
        source_groups: (2..1002u64).map(|id| id.to_be_bytes().to_vec()).collect(),
    };
    let request = mailbox_request::Request::ReplicaPut(batch);
    assert_eq!(put(&service, &replica_peer("r"), request), Status::Ok);
    let held = service.held(&mailbox).unwrap();
    assert_eq!(held[&local.id].1, vec![7; 8]);
    assert_eq!(held[&client.id].1.len(), 8);
    assert!(held.len() < 1002);
    assert!(tokenless_bytes(&service, &mailbox) <= 101 * 1024);
    // A label over 8 bytes is stored empty.
    let long = plain(5000, &mailbox, Message, 10);
    let batch = proto::Envelopes {
        envelopes: vec![sign(&long)],
        source_groups: vec![vec![1; 9]],
    };
    let request = mailbox_request::Request::ReplicaPut(batch);
    assert_eq!(put(&service, &replica_peer("r"), request), Status::Ok);
    assert!(service.held(&mailbox).unwrap()[&long.id].1.is_empty());
}

#[test]
fn the_store_wide_share_evicts_tokenless_signals_and_with_tokens_messages() {
    use mailbox_role::{Group, Source};
    use proto::EnvelopeClass::{Message, Signal};
    for tokens in [false, true] {
        let service = pooled_service(|l| {
            l.require_write_tokens = tokens;
            l.messages_max_mb = 1;
            l.tokenless_share_pct = 1;
        });
        let share = (1 << 20) / 100;
        let sender = Identity::generate();
        let mailboxes: Vec<_> = (0..3).map(|_| mailbox_of(&Identity::generate())).collect();
        let mut id = 0;
        for (mailbox, class, len) in [
            (&mailboxes[0], Signal, 600),
            (&mailboxes[1], Signal, 600),
            (&mailboxes[2], Message, 1_000),
        ] {
            for _ in 0..12 {
                id += 1;
                let envelope = plain(id, mailbox, class, len);
                let record = sender.sign(Domain::Envelope, envelope.encode_to_vec());
                let response = service.put(
                    &record,
                    true,
                    Source::Client,
                    MAX_ENVELOPE_BYTES,
                    "p",
                    Group::Ip("a"),
                );
                assert_eq!(mailbox_status(&response.unwrap()), Status::Ok);
            }
        }
        let signals =
            tokenless_bytes(&service, &mailboxes[0]) + tokenless_bytes(&service, &mailboxes[1]);
        let messages = tokenless_bytes(&service, &mailboxes[2]);
        if tokens {
            assert!(signals + messages <= share, "{signals} + {messages}");
        } else {
            assert!(signals <= share, "{signals}");
            assert_eq!(service.held_ids(&mailboxes[2]).unwrap().len(), 12);
        }
    }
}

#[test]
fn a_tokened_envelope_fits_the_same_100_kib() {
    let (owner, contact) = (Identity::generate(), Identity::generate());
    let mailbox = mailbox_of(&owner);
    let mut tokened = envelope(1, &mailbox, token(&owner, &mailbox, &contact, today()));
    tokened.ciphertext = vec![0; MAX_ENVELOPE_BYTES];
    let overhead = tokened.encoded_len() - tokened.ciphertext.len();
    tokened.ciphertext.resize(MAX_ENVELOPE_BYTES - overhead, 0);
    assert_eq!(tokened.encoded_len(), MAX_ENVELOPE_BYTES);
    let service = service();
    let request =
        mailbox_request::Request::Put(contact.sign(Domain::Envelope, tokened.encode_to_vec()));
    assert_eq!(put(&service, &peer("p"), request), Status::Ok);
    tokened.ciphertext.push(0);
    let request = MailboxRequest {
        request: Some(mailbox_request::Request::Put(
            contact.sign(Domain::Envelope, tokened.encode_to_vec()),
        )),
    };
    let response = service.mailbox(&peer("p"), &mut None, request).unwrap();
    assert_eq!(
        (mailbox_status(&response), response.max_bytes),
        (Status::TooLarge, MAX_ENVELOPE_BYTES as u64)
    );
}
