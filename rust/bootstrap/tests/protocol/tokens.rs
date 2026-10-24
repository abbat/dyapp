//! Sending with a held write token, and the fallbacks when the node refuses it.
use super::chat::{store_path, Person};
use super::*;
use dyapp_client::store::Store;
use dyapp_client::tokens;

const SEAL: [u8; 32] = [3; 32];

/// Makes `contact` hold a token `signer` signs for `mailbox` on `epoch`; returns its key.
fn hold(contact: &mut Store, signer: &Identity, mailbox: &[u8; 32], epoch: u32) -> [u8; 32] {
    let key = contact.token_key(mailbox, epoch, &SEAL).unwrap();
    let token = proto::WriteToken {
        mailbox: mailbox.to_vec(),
        token_key: key.to_vec(),
        epoch,
    };
    let signed = signer.sign(Domain::WriteToken, token.encode_to_vec());
    contact
        .set_write_token(mailbox, epoch, &signed.encode_to_vec())
        .unwrap();
    key
}

#[tokio::test]
async fn a_held_token_is_used_and_a_refused_one_falls_back() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut owner = Person::new(&address).await;
    let mut contact = Person::new(&address).await;
    let mut store = Store::open(&store_path("contact"), &contact.identity).unwrap();
    let mailbox = owner.mailbox();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let today = (now / 86_400) as u32;
    // A put's sender key: the token key, or a one-off key for a tokenless write.
    let mut sent = async |store: &Store, day: u32| {
        let epoch = tokens::send(&mut contact.node, store, &mailbox, &[1], day, &SEAL)
            .await
            .unwrap();
        let (envelopes, _) = owner.node.fetch(&owner.device_key).await.unwrap();
        let ids = envelopes
            .iter()
            .map(|envelope| envelope.id.clone())
            .collect();
        owner.node.ack(&owner.device_key, ids).await.unwrap();
        assert_eq!(envelopes.len(), 1);
        (epoch, envelopes[0].sender_key.clone())
    };

    // No token held: tokenless.
    assert_eq!(sent(&store, today).await.0, None);

    // Today's token from the owner device is taken.
    let key = hold(&mut store, &owner.device_key, &mailbox, today);
    assert_eq!(sent(&store, today).await, (Some(today), key.to_vec()));

    // A token signed by someone else is DENIED and the write goes tokenless.
    let forged = hold(&mut store, &Identity::generate(), &mailbox, today + 3);
    let (epoch, sender) = sent(&store, today + 3).await;
    assert_eq!(epoch, None);
    assert_ne!(sender, forged.to_vec());

    // Days ahead of the node are STALE, the next one too: tokenless.
    for day in [today + 5, today + 6] {
        hold(&mut store, &owner.device_key, &mailbox, day);
    }
    assert_eq!(sent(&store, today + 5).await.0, None);

    // A device a day behind is STALE once the node's day is an hour old, then takes the next day.
    hold(&mut store, &owner.device_key, &mailbox, today - 1);
    let expected = if now % 86_400 < 3600 {
        today - 1
    } else {
        today
    };
    assert_eq!(sent(&store, today - 1).await.0, Some(expected));
}

#[tokio::test]
async fn a_revoked_contact_token_is_denied_and_the_write_goes_tokenless() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut owner = Person::new(&address).await;
    let mut contact = Person::new(&address).await;
    let owner_store = Store::open(&store_path("revoking-owner"), &owner.identity).unwrap();
    let mut store = Store::open(&store_path("revoked-contact"), &contact.identity).unwrap();
    let mailbox = owner.mailbox();
    let today = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 86_400) as u32;
    let key = hold(&mut store, &owner.device_key, &mailbox, today);
    owner_store
        .record_issued("contact", &mailbox, &key, today)
        .unwrap();
    assert_eq!(
        put(&mut owner, &mut contact, &store, today).await,
        Some(today)
    );

    let revoked = tokens::revoke(
        &mut owner.node,
        &owner_store,
        &owner.device_key,
        "contact",
        today,
    )
    .await
    .unwrap();
    assert_eq!(revoked, 1);
    assert!(owner_store.issued_revoked(&key).unwrap());
    assert_eq!(put(&mut owner, &mut contact, &store, today).await, None);
}

/// Sends one byte from `contact` to `owner` with `tokens::send` and fetches it; returns the day
/// of the token the node took.
async fn put(owner: &mut Person, contact: &mut Person, store: &Store, today: u32) -> Option<u32> {
    let mailbox = owner.mailbox();
    let epoch = tokens::send(&mut contact.node, store, &mailbox, &[1], today, &SEAL)
        .await
        .unwrap();
    assert_eq!(senders(owner).await.len(), 1);
    epoch
}

/// Fetches and acks `owner`'s mailbox; returns each envelope's sender key.
async fn senders(owner: &mut Person) -> Vec<Vec<u8>> {
    let (envelopes, _) = owner.node.fetch(&owner.device_key).await.unwrap();
    let ids = envelopes.iter().map(|e| e.id.clone()).collect();
    owner.node.ack(&owner.device_key, ids).await.unwrap();
    envelopes.into_iter().map(|e| e.sender_key).collect()
}

#[tokio::test]
async fn a_session_sends_with_a_held_token_and_swaps_a_revoked_one_in_its_outbox() {
    use dyapp_client::{chat, session::Session};

    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    let mut profiles = Vec::new();
    for owner in [alice.identity.peer_id(), bob_owner] {
        profiles.push(alice.node.get_profile(&owner).await.unwrap());
    }
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
    assert_eq!(senders(&mut bob).await.len(), 1);
    let group_id = group.id().to_vec();
    let today = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 86_400) as u32;
    let mut store = Store::open(&store_path("token-session"), &alice.identity).unwrap();
    store
        .stage_group(
            &group,
            &profiles[1],
            &alice.device.seal(&SEAL).unwrap(),
            &[],
        )
        .unwrap();
    drop(group);
    let bob_mailbox = bob.mailbox();
    let key = hold(&mut store, &bob.device_key, &bob_mailbox, today);
    let mut session = Session::open(store, &SEAL).unwrap();

    session
        .send_message(&mut alice.node, &group_id, b"one", &SEAL)
        .await
        .unwrap();
    assert_eq!(senders(&mut bob).await, [key.to_vec()]);

    // Bob revokes the key: the refused write is swapped in the outbox and goes tokenless.
    let bob_store = Store::open(&store_path("token-session-bob"), &bob.identity).unwrap();
    bob_store
        .record_issued("alice", &bob_mailbox, &key, today)
        .unwrap();
    tokens::revoke(&mut bob.node, &bob_store, &bob.device_key, "alice", today)
        .await
        .unwrap();
    session
        .send_message(&mut alice.node, &group_id, b"two", &SEAL)
        .await
        .unwrap();
    let sent = senders(&mut bob).await;
    assert_eq!(sent.len(), 1);
    assert_ne!(sent[0], key.to_vec());
    assert_eq!(session.due_outbound(i64::MAX).unwrap().len(), 2);
}

async fn receive(
    session: &mut dyapp_client::session::Session,
    person: &mut Person,
    profiles: &[dyapp_profile::VerifiedProfile],
) -> Vec<dyapp_client::chat::Event> {
    session
        .receive(&mut person.node, &person.device_key, profiles, &SEAL)
        .await
        .unwrap()
}

#[tokio::test]
async fn two_sessions_trade_tokens_on_join_and_a_message_passes_a_full_pool() {
    use dyapp_client::{chat, session::Session};

    // Default limits: require_write_tokens is on.
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    let mut profiles = Vec::new();
    for owner in [alice.identity.peer_id(), bob_owner] {
        profiles.push(alice.node.get_profile(&owner).await.unwrap());
    }
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
    let (alice_path, bob_path) = (store_path("trade-alice"), store_path("trade-bob"));
    let mut store = Store::open(&alice_path, &alice.identity).unwrap();
    let sealed = alice.device.seal(&SEAL).unwrap();
    store
        .stage_group(&group, &profiles[1], &sealed, &[])
        .unwrap();
    let mut alice_session = Session::open(store, &SEAL).unwrap();
    let mut store = Store::open(&bob_path, &bob.identity).unwrap();
    store
        .stage_outbound(&bob.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    let mut bob_session = Session::open(store, &SEAL).unwrap();

    // Bob joins and asks; Alice answers and asks back; Bob stores, answers; Alice stores.
    let joined = receive(&mut bob_session, &mut bob, &profiles).await;
    assert_eq!(joined, [chat::Event::Joined(group.id().to_vec())]);
    assert!(receive(&mut alice_session, &mut alice, &profiles)
        .await
        .is_empty());
    assert!(receive(&mut bob_session, &mut bob, &profiles)
        .await
        .is_empty());
    assert!(receive(&mut alice_session, &mut alice, &profiles)
        .await
        .is_empty());

    // A stranger fills Bob's tokenless pool; Alice's tokened message still reaches him.
    let mut stranger = Person::new(&address).await;
    for _ in 0..12 {
        stranger
            .node
            .put(&bob.mailbox(), vec![7; 90_000])
            .await
            .unwrap();
    }
    alice_session
        .send_message(&mut alice.node, group.id(), b"hi", &SEAL)
        .await
        .unwrap();
    let mut events = Vec::new();
    for _ in 0..5 {
        events.extend(receive(&mut bob_session, &mut bob, &profiles).await);
    }
    assert!(events.iter().any(|event| matches!(event,
        chat::Event::Message { body: chat::Body::Text(text), .. } if text == b"hi")));
    drop((alice_session, bob_session));

    let today = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 86_400) as u32;
    let month = (today..today + 30).collect::<Vec<_>>();
    let alice_store = Store::open(&alice_path, &alice.identity).unwrap();
    let bob_store = Store::open(&bob_path, &bob.identity).unwrap();
    assert_eq!(
        alice_store
            .write_token_epochs(&bob.mailbox(), today)
            .unwrap(),
        month
    );
    assert_eq!(
        bob_store.write_token_epochs(&alice_mailbox, today).unwrap(),
        month
    );
}
