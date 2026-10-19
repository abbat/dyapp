//! Write token checks on client put, replica put and repair.
use super::*;
use mailbox_role::{check_token, TokenError};

const DAY: i64 = 86_400;

fn today() -> u32 {
    (chrono::Utc::now().timestamp() / DAY) as u32
}

/// A token for `owner`'s mailbox handed to `token_key` on `epoch`.
fn token(owner: &Identity, mailbox: &[u8], token_key: &Identity, epoch: u32) -> SignedRecord {
    let payload = proto::WriteToken {
        mailbox: mailbox.to_vec(),
        token_key: token_key.public_key().to_vec(),
        epoch,
    };
    owner.sign(Domain::WriteToken, payload.encode_to_vec())
}

fn envelope(id: u8, mailbox: &[u8], token: SignedRecord) -> proto::Envelope {
    proto::Envelope {
        id: vec![id; 16],
        mailbox: mailbox.to_vec(),
        ciphertext: vec![1],
        write_token: Some(token),
        ..Default::default()
    }
}

fn put(service: &Service, from: &Peer, request: mailbox_request::Request) -> Status {
    let request = MailboxRequest {
        request: Some(request),
    };
    mailbox_status(&service.mailbox(from, &mut None, request).unwrap())
}

fn mailbox_of(owner: &Identity) -> Vec<u8> {
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
