//! Write token revocation: `revoke`, `replica_revoke` and the puts it refuses.
use super::write_token::{envelope, mailbox_of, put, today, token};
use super::*;

fn revoke(owner: &Identity, mailbox: &[u8], keys: &[&Identity], created_at: i64) -> SignedRecord {
    let payload = proto::Revoke {
        mailbox: mailbox.to_vec(),
        token_keys: keys.iter().map(|key| key.public_key().to_vec()).collect(),
        created_at,
        owner: None,
    };
    owner.sign(Domain::MailboxRevoke, payload.encode_to_vec())
}

/// A revoke by `identity` closing the mailbox of `device`, which accepted `owner`.
fn close(identity: &Identity, owner: &Identity, device: &Identity) -> SignedRecord {
    let payload = proto::Revoke {
        mailbox: mailbox_of(device),
        created_at: chrono::Utc::now().timestamp(),
        owner: Some(device.sign(Domain::DeviceOwner, owner.public_key().to_vec())),
        ..Default::default()
    };
    identity.sign(Domain::MailboxRevoke, payload.encode_to_vec())
}

#[test]
fn a_revoke_deletes_the_keys_envelopes_and_refuses_their_puts_without_a_strike() {
    let (owner, contact, other) = (
        Identity::generate(),
        Identity::generate(),
        Identity::generate(),
    );
    let mailbox = mailbox_of(&owner);
    let service = service_with(|l| l.strikes_to_ban = 1);
    let signed = |id: u8, by: &Identity| {
        let token = token(&owner, &mailbox, by, today());
        by.sign(
            Domain::Envelope,
            envelope(id, &mailbox, token).encode_to_vec(),
        )
    };
    let client = peer("p");
    for (id, by) in [(1, &contact), (2, &other)] {
        let request = mailbox_request::Request::Put(signed(id, by));
        assert_eq!(put(&service, &client, request), Status::Ok);
    }
    let now = chrono::Utc::now().timestamp();
    let record = revoke(&owner, &mailbox, &[&contact], now);
    let request = mailbox_request::Request::Revoke(record.clone());
    assert_eq!(put(&service, &client, request), Status::Ok);
    assert_eq!(service.held_ids(&mailbox).unwrap(), vec![vec![2; 16]]);
    let request = mailbox_request::Request::Put(signed(3, &contact));
    assert_eq!(put(&service, &client, request), Status::Denied);
    assert!(!service.reputation.banned(&client.id));
    let replica = replica_peer("replica");
    let batch = proto::Envelopes {
        envelopes: vec![signed(4, &contact)],
        ..Default::default()
    };
    let request = mailbox_request::Request::ReplicaPut(batch);
    assert_eq!(put(&service, &replica, request), Status::Ok);
    assert_eq!(service.held_ids(&mailbox).unwrap(), vec![vec![2; 16]]);

    // Repair carries the signed record to another replica as is.
    assert_eq!(service.revokes(&mailbox).unwrap(), [record]);
    let next = service_with(|_| {});
    let record = service.revokes(&mailbox).unwrap().remove(0);
    let request = mailbox_request::Request::ReplicaRevoke(record);
    assert_eq!(put(&next, &replica, request), Status::Ok);
    let key = contact.public_key();
    assert!(next.revoked(&mailbox, &key).unwrap());
    assert!(!next.revoked(&mailbox, &other.public_key()).unwrap());
}

#[test]
fn forged_revokes_strike_and_future_or_expired_ones_do_not() {
    let (owner, contact, stranger) = (
        Identity::generate(),
        Identity::generate(),
        Identity::generate(),
    );
    let mailbox = mailbox_of(&owner);
    let service = service_with(|l| l.strikes_to_ban = 1);
    let now = chrono::Utc::now().timestamp();
    let send = |from: &str, record: SignedRecord| {
        put(
            &service,
            &peer(from),
            mailbox_request::Request::Revoke(record),
        )
    };
    let expired = now - 31 * 86_400 - service.retention() as i64 - 3600;
    assert_eq!(
        send("a", revoke(&owner, &mailbox, &[&contact], now + 3601)),
        Status::Denied
    );
    assert_eq!(
        send("a", revoke(&owner, &mailbox, &[&contact], expired)),
        Status::Stale
    );
    assert_eq!(
        send("a", revoke(&owner, &mailbox, &[], now)),
        Status::Invalid
    );
    assert!(!service.reputation.banned("a"));
    assert!(!service.revoked(&mailbox, &contact.public_key()).unwrap());

    // Signed by a key other than the mailbox's, or with a broken signature.
    assert_eq!(
        send("b", revoke(&stranger, &mailbox, &[&contact], now)),
        Status::Denied
    );
    assert!(service.reputation.banned("b"));
    let mut broken = revoke(&owner, &mailbox, &[&contact], now);
    broken.signature[0] ^= 1;
    assert_eq!(send("c", broken), Status::Denied);
    assert!(service.reputation.banned("c"));
}

#[test]
fn the_owner_closes_a_revoked_device_mailbox() {
    let (identity, device, contact, stranger) = (
        Identity::generate(),
        Identity::generate(),
        Identity::generate(),
        Identity::generate(),
    );
    let mailbox = mailbox_of(&device);
    let service = service_with(|l| l.strikes_to_ban = 1);
    let signed = |id: u8| {
        let token = token(&device, &mailbox, &contact, today());
        contact.sign(
            Domain::Envelope,
            envelope(id, &mailbox, token).encode_to_vec(),
        )
    };
    let client = peer("p");
    let request = mailbox_request::Request::Put(signed(1));
    assert_eq!(put(&service, &client, request), Status::Ok);

    // Only the identity the device accepted closes its mailbox, and only that device's.
    let mut other = close(&stranger, &stranger, &contact);
    let mut payload = proto::Revoke::decode(other.payload.as_slice()).unwrap();
    payload.mailbox.clone_from(&mailbox);
    other = stranger.sign(Domain::MailboxRevoke, payload.encode_to_vec());
    for (from, record) in [("a", close(&stranger, &identity, &device)), ("b", other)] {
        let request = mailbox_request::Request::Revoke(record);
        assert_eq!(put(&service, &peer(from), request), Status::Denied);
        assert!(service.reputation.banned(from));
    }
    let record = close(&identity, &identity, &device);
    let request = mailbox_request::Request::Revoke(record.clone());
    assert_eq!(put(&service, &client, request), Status::Ok);
    assert!(service.held_ids(&mailbox).unwrap().is_empty());

    // The lost device's fetch, new puts and repair inventory are refused.
    let nonce = [5; 32];
    let fetch = device.sign(
        Domain::MailboxFetch,
        proto::Fetch {
            nonce: nonce.to_vec(),
            ..Default::default()
        }
        .encode_to_vec(),
    );
    let request = MailboxRequest {
        request: Some(mailbox_request::Request::Fetch(fetch)),
    };
    let answer = service.mailbox(&client, &mut Some(nonce), request).unwrap();
    assert_eq!(mailbox_status(&answer), Status::Refused);
    let request = mailbox_request::Request::Put(signed(2));
    assert_eq!(put(&service, &client, request), Status::Refused);
    let inventory = proto::Inventory {
        mailbox: mailbox.clone(),
        ids: vec![vec![2; 16]],
    };
    let request = mailbox_request::Request::Inventory(inventory);
    assert_eq!(put(&service, &replica_peer("r"), request), Status::Refused);
    assert!(!service.reputation.banned(&client.id));

    // Repair carries the close to another replica.
    assert_eq!(
        service.revokes(&mailbox).unwrap(),
        std::slice::from_ref(&record)
    );
    let next = service_with(|_| {});
    let request = mailbox_request::Request::ReplicaRevoke(record);
    assert_eq!(put(&next, &replica_peer("r"), request), Status::Ok);
    assert!(next.closed(&mailbox).unwrap());
}
