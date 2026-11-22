//! The last-resort `KeyPackage` outlives the single-use stock, a lost reply, a rotation and a
//! restart of the invited device.
use super::chat::Person;
use super::*;
use dyapp_client::chat;
use dyapp_client::session::Session;
use dyapp_client::store::Store;
use std::slice::from_mut;

const SEAL: [u8; 32] = [5; 32];
const ROTATE_AFTER: u64 = 21 * 86_400;

#[tokio::test]
async fn invites_work_from_one_last_resort_package_across_lost_replies_and_rotation() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
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
    let path = test_dir.path().join("last-resort-bob.db");
    let mut store = Store::open(&path, &bob.identity).unwrap();
    store
        .stage_outbound(&bob.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    let mut session = Session::open(store, &SEAL).unwrap();
    let now = dyapp_bootstrap::clock::Clock::system()
        .timestamp()
        .unsigned_abs();
    assert_repeats_after_lost_reply(&mut session, &mut bob, now).await;
    let profile = assert_rotation(&mut session, &mut bob, &mut alice, &owner, now).await;
    drop(session);
    receive_rotated_welcome(&path, &mut bob, &mut alice, profile).await;
}

async fn keep_last_resort(session: &mut Session, bob: &mut Person, now: u64) {
    session
        .keep_last_resort(&mut bob.node, &bob.device_key, now, &SEAL)
        .await
        .unwrap();
}

async fn assert_repeats_after_lost_reply(session: &mut Session, bob: &mut Person, now: u64) {
    keep_last_resort(session, bob, now).await;
    let first = session.store().last_resort().unwrap().unwrap();
    assert!(first.published);
    // Nothing is due, and an unconfirmed record repeats as it is.
    keep_last_resort(session, bob, now).await;
    assert_eq!(
        session.store().last_resort().unwrap().as_ref(),
        Some(&first)
    );
    let mut lost = first.clone();
    lost.published = false;
    session.store().save_last_resort(&lost).unwrap();
    keep_last_resort(session, bob, now).await;
    assert_eq!(
        session.store().last_resort().unwrap().as_ref(),
        Some(&first)
    );
}

async fn assert_rotation(
    session: &mut Session,
    bob: &mut Person,
    alice: &mut Person,
    owner: &str,
    now: u64,
) -> dyapp_profile::VerifiedProfile {
    let first = session.store().last_resort().unwrap().unwrap();
    // The node keeps serving the same package to every inviter.
    let profile = alice.node.get_profile(owner).await.unwrap();
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
    keep_last_resort(session, bob, now + ROTATE_AFTER).await;
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
    profile
}

async fn receive_rotated_welcome(
    path: &std::path::Path,
    bob: &mut Person,
    alice: &mut Person,
    profile: dyapp_profile::VerifiedProfile,
) {
    // A restarted device still opens a Welcome made from the rotated package.
    let mut session = Session::open(Store::open(path, &bob.identity).unwrap(), &SEAL).unwrap();
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
