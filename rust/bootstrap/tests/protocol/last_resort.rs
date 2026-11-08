//! The last-resort `KeyPackage` outlives the single-use stock, a lost reply, a rotation and a
//! restart of the invited device.
use super::chat::{store_path, Person};
use super::*;
use dyapp_client::chat;
use dyapp_client::session::Session;
use dyapp_client::store::Store;
use std::slice::from_mut;

const SEAL: [u8; 32] = [5; 32];
const ROTATE_AFTER: u64 = 21 * 86_400;

#[tokio::test]
async fn invites_work_from_one_last_resort_package_across_lost_replies_and_rotation() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
        config.limits.require_write_tokens = false;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let owner = bob.identity.peer_id();
    let path = store_path("last-resort-bob");
    let mut store = Store::open(&path, &bob.identity).unwrap();
    store
        .stage_outbound(&bob.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    let mut session = Session::open(store, &SEAL).unwrap();
    let now = chrono::Utc::now().timestamp().unsigned_abs();

    session
        .keep_last_resort(&mut bob.node, &bob.device_key, now, &SEAL)
        .await
        .unwrap();
    let first = session.store().last_resort().unwrap().unwrap();
    assert!(first.published);

    // Nothing is due, and an unconfirmed record repeats as it is.
    session
        .keep_last_resort(&mut bob.node, &bob.device_key, now, &SEAL)
        .await
        .unwrap();
    assert_eq!(
        session.store().last_resort().unwrap().as_ref(),
        Some(&first)
    );
    let mut lost = first.clone();
    lost.published = false;
    session.store().save_last_resort(&lost).unwrap();
    session
        .keep_last_resort(&mut bob.node, &bob.device_key, now, &SEAL)
        .await
        .unwrap();
    assert_eq!(
        session.store().last_resort().unwrap().as_ref(),
        Some(&first)
    );

    // The node keeps serving the same package to every inviter.
    let profile = alice.node.get_profile(&owner).await.unwrap();
    let mailbox = bob.mailbox();
    let package = alice
        .node
        .fetch_key_package(&profile, &mailbox)
        .await
        .unwrap();
    assert_eq!(
        alice
            .node
            .fetch_key_package(&profile, &mailbox)
            .await
            .unwrap(),
        package
    );

    let due = now + ROTATE_AFTER;
    session
        .keep_last_resort(&mut bob.node, &bob.device_key, due, &SEAL)
        .await
        .unwrap();
    let second = session.store().last_resort().unwrap().unwrap();
    assert!(second.published && second.revision > first.revision);
    assert_ne!(
        alice
            .node
            .fetch_key_package(&profile, &mailbox)
            .await
            .unwrap(),
        package
    );

    // A restarted device still opens a Welcome made from the rotated package.
    drop(session);
    let mut session = Session::open(Store::open(&path, &bob.identity).unwrap(), &SEAL).unwrap();
    let profiles = vec![
        alice
            .node
            .get_profile(&alice.identity.peer_id())
            .await
            .unwrap(),
        profile,
    ];
    let alice_mailbox = alice.mailbox();
    let (_group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
    session
        .receive(from_mut(&mut bob.node), &bob.device_key, &profiles, &SEAL)
        .await
        .unwrap();
    assert_eq!(session.store().known_groups().unwrap().len(), 1);
}
