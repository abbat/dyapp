//! Mailbox, deny-list, inventory, quota and rate-limit tests.
use super::*;

fn mailbox(
    service: &Service,
    nonce: &mut Option<[u8; 32]>,
    request: mailbox_request::Request,
) -> MailboxResponse {
    let sender = if matches!(
        request,
        mailbox_request::Request::ReplicaPut(_) | mailbox_request::Request::Inventory(_)
    ) {
        replica_peer("replica")
    } else {
        peer("p")
    };
    let request = MailboxRequest {
        request: Some(request),
    };
    service.mailbox(&sender, nonce, request).unwrap()
}

fn challenge(service: &Service, nonce: &mut Option<[u8; 32]>) -> Vec<u8> {
    let request = mailbox_request::Request::Challenge(proto::ChallengeRequest {});
    mailbox(service, nonce, request).nonce
}

fn fetch(device: &Identity, nonce: Vec<u8>) -> mailbox_request::Request {
    let fetch = proto::Fetch {
        nonce,
        ..proto::Fetch::default()
    };
    mailbox_request::Request::Fetch(device.sign(Domain::MailboxFetch, fetch.encode_to_vec()))
}

pub(super) fn mailbox_status(response: &MailboxResponse) -> Status {
    Status::try_from(response.status).unwrap()
}

#[test]
fn deny_list_refuses_without_strikes() {
    let mut service = service_with(|l| l.strikes_to_ban = 1);
    let (listed, device) = (Identity::generate(), Identity::generate());
    let address = dyapp_identity::key_hash(&device.public_key()).to_vec();
    let listed_id = dyapp_identity::key_hash(&listed.public_key()).to_vec();
    *service.deny.get_mut().unwrap() =
        [hex(&listed_id), hex(&address), "bad".into(), "g6".into()].into();
    let profile = |p: &str, request| status(&service.profile(&peer(p), request).unwrap());
    let record = Profile::default().sign(&listed);
    assert_eq!(profile("p", publish(record)), Status::Refused);
    assert_eq!(profile("p", get(listed_id)), Status::Refused);
    assert_eq!(profile("bad", get(vec![1; 32])), Status::Refused);
    let group = Peer {
        id: "q".into(),
        group: "g6".into(),
    };
    let response = service.profile(&group, get(vec![1; 32])).unwrap();
    assert_eq!(status(&response), Status::Refused);
    let envelope = proto::Envelope {
        id: vec![1; 16],
        mailbox: address,
        ciphertext: vec![1],
        ..Default::default()
    };
    let to_listed = Identity::generate().sign(Domain::Envelope, envelope.encode_to_vec());
    let put = mailbox_request::Request::Put(to_listed);
    assert_eq!(
        mailbox_status(&mailbox(&service, &mut None, put)),
        Status::Refused
    );
    assert!(!service.reputation.banned("p"));
    assert_eq!(profile("p", get(vec![1; 32])), Status::NotFound);
}

#[test]
fn inventory_names_gaps_and_replica_put_fills_them() {
    let service = service_with(|l| l.strikes_to_ban = 1);
    let sender = Identity::generate();
    let address = vec![7; 32];
    let signed = |id: u8| {
        let envelope = proto::Envelope {
            id: vec![id; 16],
            mailbox: address.clone(),
            ciphertext: vec![id],
            ..Default::default()
        };
        sender.sign(Domain::Envelope, envelope.encode_to_vec())
    };
    let put = mailbox_request::Request::Put(signed(1));
    assert_eq!(
        mailbox_status(&mailbox(&service, &mut None, put)),
        Status::Ok
    );
    let inventory = |ids: Vec<Vec<u8>>| {
        let request = mailbox_request::Request::Inventory(proto::Inventory {
            mailbox: address.clone(),
            ids,
        });
        mailbox(&service, &mut None, request)
    };
    let response = inventory(vec![vec![1; 16], vec![2; 16]]);
    assert_eq!(response.missing, [vec![2; 16]]);
    assert_eq!(
        mailbox_status(&inventory(vec![vec![2; 3]])),
        Status::Invalid
    );
    let batch = |envelopes| {
        let request = mailbox_request::Request::ReplicaPut(proto::Envelopes { envelopes });
        mailbox_status(&mailbox(&service, &mut None, request))
    };
    assert_eq!(batch(vec![signed(2)]), Status::Ok);
    assert!(inventory(vec![vec![2; 16]]).missing.is_empty());
    let mut forged = signed(3);
    forged.signature[0] ^= 1;
    assert_eq!(batch(vec![signed(4), forged]), Status::Denied);
    assert_eq!(service.held(&address).unwrap().len(), 3);
    assert_eq!(service.held_ids(&address).unwrap().len(), 3);
    // A client asking for an inventory is refused before any read, with a strike.
    let client = service
        .mailbox(
            &peer("p"),
            &mut None,
            MailboxRequest {
                request: Some(mailbox_request::Request::Inventory(proto::Inventory {
                    mailbox: address.clone(),
                    ids: vec![vec![9; 16]],
                })),
            },
        )
        .unwrap();
    assert_eq!(mailbox_status(&client), Status::Denied);
    assert!(client.missing.is_empty());
    assert!(service.reputation.banned("p"));
}

#[test]
fn mailbox_size_hints_cover_boundaries_local_replicas_and_denied_batch_priority() {
    let s = service();
    let owner = Identity::generate();
    let mut envelope = proto::Envelope {
        id: vec![4; 16],
        mailbox: vec![7; 32],
        ciphertext: vec![0; MAX_ENVELOPE_BYTES],
        ..Default::default()
    };
    let overhead = envelope.encoded_len() - envelope.ciphertext.len();
    envelope.ciphertext.resize(MAX_ENVELOPE_BYTES - overhead, 0);
    let payload = envelope.encode_to_vec();
    assert_eq!(payload.len(), MAX_ENVELOPE_BYTES);
    let exact = owner.sign(Domain::Envelope, payload);
    let response = s.put(&exact, false, false, MAX_ENVELOPE_BYTES - 1).unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::TooLarge), (MAX_ENVELOPE_BYTES - 1) as u64)
    );
    let response = s
        .mailbox(
            &peer("limits"),
            &mut None,
            MailboxRequest {
                request: Some(mailbox_request::Request::Put(exact.clone())),
            },
        )
        .unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::Ok), 0)
    );
    let response = s.store_mailbox(&exact).unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::Ok), 0)
    );
    envelope.ciphertext.push(0);
    let large = owner.sign(Domain::Envelope, envelope.encode_to_vec());
    assert_eq!(large.payload.len(), MAX_ENVELOPE_BYTES + 1);
    let response = s
        .mailbox(
            &peer("limits"),
            &mut None,
            MailboxRequest {
                request: Some(mailbox_request::Request::Put(large.clone())),
            },
        )
        .unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::TooLarge), MAX_ENVELOPE_BYTES as u64)
    );
    let response = s.store_mailbox(&large).unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::TooLarge), ENVELOPE_ACCEPT_BYTES as u64)
    );
    let from = replica_peer("limit-node");
    let response = s
        .mailbox(
            &from,
            &mut None,
            MailboxRequest {
                request: Some(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                    envelopes: vec![large.clone(), exact.clone()],
                })),
            },
        )
        .unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::TooLarge), ENVELOPE_ACCEPT_BYTES as u64)
    );
    let mut forged = exact;
    forged.signature[0] ^= 1;
    for envelopes in [vec![large.clone(), forged.clone()], vec![forged, large]] {
        let response = s
            .mailbox(
                &from,
                &mut None,
                MailboxRequest {
                    request: Some(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                        envelopes,
                    })),
                },
            )
            .unwrap();
        assert_eq!(
            (response.status, response.max_bytes),
            (i32::from(Status::Denied), 0)
        );
    }
}

#[test]
fn mailbox_put_fetch_ack_forged_and_replayed() {
    let service = service();
    let (device, sender, thief) = (
        Identity::generate(),
        Identity::generate(),
        Identity::generate(),
    );
    let address = dyapp_identity::key_hash(&device.public_key()).to_vec();
    let envelope = |id: Vec<u8>, ciphertext| proto::Envelope {
        id,
        mailbox: address.clone(),
        ciphertext,
        ..Default::default()
    };
    let put = |envelope: proto::Envelope| {
        mailbox_request::Request::Put(sender.sign(Domain::Envelope, envelope.encode_to_vec()))
    };
    let (mut conn, mut other) = (None, None);

    let signed = sender.sign(
        Domain::Envelope,
        envelope(vec![7; 16], vec![1]).encode_to_vec(),
    );
    let response = mailbox(
        &service,
        &mut conn,
        mailbox_request::Request::Put(signed.clone()),
    );
    assert_eq!(mailbox_status(&response), Status::Ok);
    let mut forged = signed.clone();
    forged.payload.push(0);
    let response = mailbox(&service, &mut conn, mailbox_request::Request::Put(forged));
    assert_eq!(mailbox_status(&response), Status::Denied);
    let wrong_domain = sender.sign(Domain::Profile, signed.payload.clone());
    let response = mailbox(
        &service,
        &mut conn,
        mailbox_request::Request::Put(wrong_domain),
    );
    assert_eq!(mailbox_status(&response), Status::Denied);
    let response = mailbox(&service, &mut conn, put(envelope(vec![7; 5], vec![1])));
    assert_eq!(mailbox_status(&response), Status::Invalid);
    let large = envelope(vec![8; 16], vec![0; MAX_ENVELOPE_BYTES]);
    assert_eq!(
        mailbox_status(&mailbox(&service, &mut conn, put(large))),
        Status::TooLarge
    );

    // No challenge yet, then a nonce issued on another connection.
    let response = mailbox(&service, &mut conn, fetch(&device, vec![0; 32]));
    assert_eq!(mailbox_status(&response), Status::Denied);
    let foreign = challenge(&service, &mut other);
    let response = mailbox(&service, &mut conn, fetch(&device, foreign));
    assert_eq!(mailbox_status(&response), Status::Denied);

    // A fetch signed as an ack, then the real fetch and its replay.
    let nonce = challenge(&service, &mut conn);
    let ack_signed = proto::Fetch {
        nonce: nonce.clone(),
        ..proto::Fetch::default()
    };
    let cross = device.sign(Domain::MailboxAck, ack_signed.encode_to_vec());
    let response = mailbox(&service, &mut conn, mailbox_request::Request::Fetch(cross));
    assert_eq!(mailbox_status(&response), Status::Denied);
    let nonce = challenge(&service, &mut conn);
    let request = fetch(&device, nonce);
    let response = mailbox(&service, &mut conn, request.clone());
    assert_eq!(
        (mailbox_status(&response), response.envelopes, response.more),
        (Status::Ok, vec![signed], false)
    );
    assert_eq!(
        mailbox_status(&mailbox(&service, &mut conn, request)),
        Status::Denied
    );

    // Another key reads and acks only its own, empty mailbox.
    let nonce = challenge(&service, &mut conn);
    let response = mailbox(&service, &mut conn, fetch(&thief, nonce));
    assert_eq!(
        (mailbox_status(&response), response.envelopes.len()),
        (Status::Ok, 0)
    );
    let ack = |key: &Identity, nonce| {
        let ack = proto::Ack {
            nonce,
            ids: vec![vec![7; 16]],
        };
        mailbox_request::Request::Ack(key.sign(Domain::MailboxAck, ack.encode_to_vec()))
    };
    let nonce = challenge(&service, &mut conn);
    assert_eq!(
        mailbox_status(&mailbox(&service, &mut conn, ack(&thief, nonce))),
        Status::Ok
    );
    let nonce = challenge(&service, &mut conn);
    assert_eq!(
        mailbox(&service, &mut conn, fetch(&device, nonce))
            .envelopes
            .len(),
        1
    );

    let nonce = challenge(&service, &mut conn);
    let request = ack(&device, nonce);
    assert_eq!(
        mailbox_status(&mailbox(&service, &mut conn, request.clone())),
        Status::Ok
    );
    assert_eq!(
        mailbox_status(&mailbox(&service, &mut conn, request)),
        Status::Denied
    );
    let nonce = challenge(&service, &mut conn);
    assert!(mailbox(&service, &mut conn, fetch(&device, nonce))
        .envelopes
        .is_empty());

    // An ack forwarded by another replica needs no nonce of this connection, only the key.
    let response = mailbox(&service, &mut conn, put(envelope(vec![7; 16], vec![2])));
    assert_eq!(mailbox_status(&response), Status::Ok);
    let replica = |key: &Identity| {
        let ack = proto::Ack {
            nonce: vec![0; 32],
            ids: vec![vec![7; 16]],
        };
        mailbox_request::Request::ReplicaAck(key.sign(Domain::MailboxAck, ack.encode_to_vec()))
    };
    assert_eq!(
        mailbox_status(&mailbox(&service, &mut conn, replica(&thief))),
        Status::Ok
    );
    let nonce = challenge(&service, &mut conn);
    let left = mailbox(&service, &mut conn, fetch(&device, nonce));
    assert_eq!(left.envelopes.len(), 1);
    assert_eq!(
        mailbox_status(&mailbox(&service, &mut conn, replica(&device))),
        Status::Ok
    );
    let nonce = challenge(&service, &mut conn);
    assert!(mailbox(&service, &mut conn, fetch(&device, nonce))
        .envelopes
        .is_empty());
}

#[test]
fn full_stores_and_byte_rate_refuse_requests() {
    let identity = Identity::generate();
    let profile = || {
        publish(
            Profile {
                version: 1,
                ..Profile::default()
            }
            .sign(&identity),
        )
    };
    let envelope = proto::Envelope {
        id: vec![7; 16],
        mailbox: vec![1; 32],
        ciphertext: vec![1],
        ..Default::default()
    };
    let put =
        mailbox_request::Request::Put(identity.sign(Domain::Envelope, envelope.encode_to_vec()));

    let service = service_with(|l| (l.profiles_max_mb, l.messages_max_mb) = (0, 0));
    assert_eq!(
        status(&service.profile(&peer("p"), profile()).unwrap()),
        Status::Full
    );
    let response = mailbox(&service, &mut None, put.clone());
    assert_eq!(mailbox_status(&response), Status::Full);
    let service = service_with(|l| l.min_free_mb = u64::MAX);
    assert_eq!(
        status(&service.profile(&peer("p"), profile()).unwrap()),
        Status::Full
    );

    // The byte rate sheds profiles before the mailbox within a second.
    let service = service_with(|l| l.bytes_per_second = 1 << 30);
    service.traffic.add((1 << 30) / 100 * 92);
    let limited = status(&service.profile(&peer("p"), get(vec![0; 32])).unwrap());
    assert_eq!(limited, Status::RateLimited);
    let response = mailbox(&service, &mut None, put.clone());
    assert_eq!(
        mailbox_status(&response),
        Status::Ok,
        "mailbox shed before the limit"
    );
    service.traffic.add(1 << 30);
    let response = mailbox(&service, &mut None, put.clone());
    assert_eq!(mailbox_status(&response), Status::RateLimited);
    assert_eq!(service.refused[5].load(Ordering::Relaxed), 2);
    service.report();

    // Memory: refused from 80 % of max_memory_mb, without a strike; dropped from 100 %.
    let service = service_with(|l| l.strikes_to_ban = 1);
    service.memory.set(85);
    let response = mailbox(&service, &mut None, put.clone());
    assert_eq!(mailbox_status(&response), Status::RateLimited);
    assert!(!service.overloaded() && !service.reputation.banned("p"));
    service.memory.set(100);
    assert!(service.overloaded());
    service.memory.set(10);
    assert!(!service.overloaded());
    let response = mailbox(&service, &mut None, put);
    assert_eq!(mailbox_status(&response), Status::Ok);
}

#[test]
fn group_and_sender_quotas_and_bans() {
    let service = service_with(|l| {
        (l.ip_group_requests_per_second, l.strikes_to_ban) = (2, 2);
        l.sender_puts_per_second = 1;
    });
    let id = || get(vec![0; 32]);
    // Two peer IDs in one IP group share its quota; another group is unaffected.
    assert_eq!(
        status(&service.profile(&peer("a"), id()).unwrap()),
        Status::NotFound
    );
    assert_eq!(
        status(&service.profile(&peer("b"), id()).unwrap()),
        Status::NotFound
    );
    let limited = service.profile(&peer("c"), id()).unwrap();
    assert_eq!(status(&limited), Status::RateLimited);
    let other = Peer {
        id: "d".into(),
        group: "h".into(),
    };
    assert_eq!(
        status(&service.profile(&other, id()).unwrap()),
        Status::NotFound
    );

    let sender = Identity::generate();
    let put = |id: u8| {
        let envelope = proto::Envelope {
            id: vec![id; 16],
            mailbox: vec![1; 32],
            ciphertext: vec![1],
            ..Default::default()
        };
        mailbox_request::Request::Put(sender.sign(Domain::Envelope, envelope.encode_to_vec()))
    };
    let service = service_with(|l| {
        l.sender_puts_per_second = 1;
        l.strikes_to_ban = 2;
    });
    let sender_peer = Peer {
        id: "e".into(),
        group: "i".into(),
    };
    let put = |id| {
        let request = MailboxRequest {
            request: Some(put(id)),
        };
        mailbox_status(&service.mailbox(&sender_peer, &mut None, request).unwrap())
    };
    assert_eq!((put(1), put(2)), (Status::Ok, Status::RateLimited));

    // A second forged record bans the peer: even a valid request is refused.
    let mut forged = Profile::default().sign(&sender);
    forged.payload.push(0);
    let bad = |group: &str| Peer {
        id: "f".into(),
        group: group.into(),
    };
    for group in ["j", "k"] {
        let response = service.profile(&bad(group), publish(forged.clone()));
        assert_eq!(status(&response.unwrap()), Status::Denied);
    }
    assert!(service.reputation.banned("f"));
    let refused = service.profile(&bad("l"), id()).unwrap();
    assert_eq!(status(&refused), Status::RateLimited);
}

#[test]
fn rate_limit_and_info() {
    let mut service = service();
    service.rate_limiter = PeerRateLimiter::new(1);
    let id = vec![0; 32];
    assert_eq!(
        status(&service.profile(&peer("p"), get(id.clone())).unwrap()),
        Status::NotFound
    );
    assert_eq!(
        status(&service.profile(&peer("p"), get(id.clone())).unwrap()),
        Status::RateLimited
    );
    assert_eq!(
        status(&service.profile(&peer("q"), get(id)).unwrap()),
        Status::NotFound
    );

    let info = service.node(
        &peer("q"),
        NodeRequest {
            request: Some(node_request::Request::Info(proto::InfoRequest {})),
        },
    );
    assert_eq!(
        info.info.unwrap().roles,
        vec![i32::from(proto::Role::Store)]
    );
    let deny_list = || NodeRequest {
        request: Some(node_request::Request::DenyList(proto::DenyListRequest {})),
    };
    let not_shared = service.node(&peer("r"), deny_list());
    assert_eq!(not_shared.status, i32::from(Status::NotFound));
    *service.shared_deny.get_mut().unwrap() = Some(SignedRecord::default());
    assert!(service.node(&peer("s"), deny_list()).deny_list.is_some());
    *service.deny.get_mut().unwrap() = ["d".into()].into();
    let info = NodeRequest {
        request: Some(node_request::Request::Info(proto::InfoRequest {})),
    };
    for request in [info, deny_list()] {
        let refused = service.node(&peer("d"), request);
        assert_eq!(refused.status, i32::from(Status::Refused));
        assert!(refused.info.is_none() && refused.deny_list.is_none());
    }
    service.deny.get_mut().unwrap().clear();
    let turn = NodeRequest {
        request: Some(node_request::Request::Turn(proto::TurnRequest {})),
    };
    let unsupported = service.node(&peer("q"), turn);
    assert_eq!(unsupported.status, i32::from(Status::Unsupported));
    service.config.roles.push(Role::Turn);
    service.config.turn.secret = "secret".into();
    let credentials = service.node(&peer("t"), turn).turn.unwrap();
    assert!(credentials.username.ends_with(":t"));
    // Vector from Python: base64(hmac.new(b"secret", user, sha1).digest()).
    let known = turn_credentials(&service.config.turn, "12D3KooWpeer", 1_000_000);
    assert_eq!(known.username, "1003600:12D3KooWpeer");
    assert_eq!(known.password, "33Vkitp/KSZTCIYERO5wm1HgMoc="); // gitleaks:allow (test vector)
}

#[test]
fn signals_are_small_counted_apart_and_evict_the_oldest() {
    use proto::EnvelopeClass::{Message, Signal};
    let service = service_with(|l| {
        l.signal_max_kb = 2;
        l.sender_puts_per_second = 1000;
    });
    let (device, sender) = (Identity::generate(), Identity::generate());
    let address = dyapp_identity::key_hash(&device.public_key()).to_vec();
    let record = |id: u8, class: proto::EnvelopeClass, len| {
        let envelope = proto::Envelope {
            id: vec![id; 16],
            mailbox: address.clone(),
            ciphertext: vec![id; len],
            class: class.into(),
        };
        sender.sign(Domain::Envelope, envelope.encode_to_vec())
    };
    let mut conn = None;
    let large = mailbox_request::Request::Put(record(9, Signal, MAX_SIGNAL_BYTES));
    let response = mailbox(&service, &mut conn, large);
    assert_eq!(
        (mailbox_status(&response), response.max_bytes),
        (Status::TooLarge, MAX_SIGNAL_BYTES as u64)
    );
    // The third signal overflows the 2 KiB quota and evicts the first; the message still fits.
    let records = [
        record(1, Signal, 600),
        record(2, Signal, 600),
        record(3, Signal, 600),
        record(4, Message, 600),
    ];
    for put in &records {
        let put = mailbox_request::Request::Put(put.clone());
        let response = mailbox(&service, &mut conn, put);
        assert_eq!(mailbox_status(&response), Status::Ok);
    }
    let nonce = challenge(&service, &mut conn);
    let response = mailbox(&service, &mut conn, fetch(&device, nonce));
    let [_, second, third, message] = records;
    assert_eq!(response.envelopes, vec![message, second, third]);
}

#[test]
fn info_reports_signal_limits_only_with_the_store_role() {
    let mut service = service_with(|l| l.signal_max_kb = 3);
    service.config.roles = vec![Role::Store];
    let info = service.info();
    assert_eq!(
        (info.max_signal_bytes, info.signal_quota_bytes),
        (MAX_SIGNAL_BYTES as u64, 3 * 1024)
    );
    service.config.roles.clear();
    let info = service.info();
    assert_eq!((info.max_signal_bytes, info.signal_quota_bytes), (0, 0));
}
