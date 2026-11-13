//! Listing acceptance must survive failure to record its local completion.
use super::*;

async fn interrupted(
    f: &mut HiddenLink,
) -> (dyapp_identity::SignedRecord, Option<Vec<u8>>, Profile) {
    let store = f.store();
    store.set_link_pending(&f.state).unwrap();
    let session =
        Session::open_or_create(store, &f.laptop.identity, &f.laptop.device_key, &SEAL).unwrap();
    let certificate = session.certificate().unwrap();
    let sealed = session.store().sealed_device().unwrap();
    let fault = rusqlite::Connection::open(&f.path).unwrap();
    fault
        .execute_batch(
            "CREATE TABLE `listing_fault` (`enabled` INTEGER NOT NULL);
         INSERT INTO `listing_fault` (`enabled`) VALUES (1);
         CREATE TRIGGER `fail_listing_proof` BEFORE INSERT ON `settings`
         WHEN NEW.`key` = 'device_listed'
          AND (SELECT `enabled` FROM `listing_fault`) = 1
         BEGIN SELECT RAISE(ABORT, 'injected listing proof failure'); END;",
        )
        .unwrap();
    let first = Box::pin(resume::resume(f, session)).await;
    assert!(
        matches!(first, Err(dyapp_client::session::Error::Store(_))),
        "{first:?}"
    );
    let listed = f
        .owner
        .node
        .get_profile(&f.owner.identity.peer_id())
        .await
        .unwrap()
        .profile;
    assert!(listed.devices.contains(&certificate));
    assert_eq!(f.store().device_listed().unwrap(), 0);
    assert!(f.store().link_pending().unwrap().is_some());
    assert!(f.store().last_resort().unwrap().is_none());
    fault
        .execute("UPDATE `listing_fault` SET `enabled` = ?1", [0])
        .unwrap();
    (certificate, sealed, listed)
}

#[tokio::test]
async fn accepted_listing_retry_completes_with_the_original_certificate() {
    let mut f = Box::pin(HiddenLink::with_roles(vec![Role::Store, Role::Media])).await;
    let (certificate, _, listed) = Box::pin(interrupted(&mut f)).await;
    let session = Session::open(f.store(), &SEAL).unwrap();
    assert_eq!(session.certificate().unwrap(), certificate);
    assert!(Box::pin(resume::resume(&mut f, session)).await.unwrap());
    let held = f
        .owner
        .node
        .get_profile(&f.owner.identity.peer_id())
        .await
        .unwrap()
        .profile;
    assert_eq!(held.version, listed.version);
    Box::pin(resume::check_packages_and_retry(&mut f, &certificate)).await;
}

#[tokio::test]
async fn interrupted_listing_cannot_restore_a_subsequently_revoked_device() {
    let mut f = Box::pin(HiddenLink::with_roles(vec![Role::Store, Role::Media])).await;
    let (certificate, sealed, listed) = Box::pin(interrupted(&mut f)).await;
    let mut revoked = Profile {
        version: listed.version.checked_add(1).unwrap(),
        devices: vec![f.owner.device.certificate().clone()],
        device_owners: Vec::new(),
        ..listed
    };
    let owner = &mut f.owner;
    publish_profile_change(&mut owner.node, &owner.identity, &mut revoked)
        .await
        .unwrap();
    let session = Session::open(f.store(), &SEAL).unwrap();
    let retried = Box::pin(resume::resume(&mut f, session)).await;
    let held = f
        .owner
        .node
        .get_profile(&f.owner.identity.peer_id())
        .await
        .unwrap()
        .profile;
    assert_eq!(
        held.version, revoked.version,
        "retry must not overwrite revocation"
    );
    assert!(!held.devices.contains(&certificate));
    assert!(
        matches!(
            retried,
            Err(dyapp_client::session::Error::Store(
                dyapp_client::store::Error::LinkEnrollmentReviewRequired
            ))
        ),
        "{retried:?}"
    );
    assert_eq!(f.store().sealed_device().unwrap(), sealed);
    assert!(f.store().link_pending().unwrap().is_some());
    assert!(f.store().last_resort().unwrap().is_none());
}
