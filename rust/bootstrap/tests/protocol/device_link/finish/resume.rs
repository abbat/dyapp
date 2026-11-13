//! Recovery completes the admitted original device, never a newly generated session.
use super::*;

async fn resume(
    f: &mut HiddenLink,
    session: Session,
) -> Result<bool, dyapp_client::session::Error> {
    session
        .resume_link(
            &mut f.laptop.node,
            &f.laptop.identity,
            &f.laptop.device_key,
            2,
            &SEAL,
            1_000_000,
        )
        .await
}

async fn check_packages_and_retry(f: &mut HiddenLink, certificate: &dyapp_identity::SignedRecord) {
    let own = f
        .owner
        .node
        .get_profile(&f.owner.identity.peer_id())
        .await
        .unwrap();
    assert!(own.profile.devices.contains(certificate));
    let mailbox = dyapp_client::mailbox(&f.laptop.device_key.public_key());
    let package = f
        .owner
        .node
        .fetch_key_package(&own, &mailbox)
        .await
        .unwrap();
    assert_ne!(package, [] as [u8; 0]);
    assert!(f.store().last_resort().unwrap().unwrap().published);
    assert_eq!(f.store().device_listed().unwrap(), own.profile.version);
    let store = f.store();
    let sealed = store.sealed_device().unwrap();
    let local = store.profile_draft().unwrap();
    let session = Session::open(store, &SEAL).unwrap();
    assert_eq!(&session.certificate().unwrap(), certificate);
    assert!(!resume(f, session).await.unwrap());
    assert_eq!(f.store().sealed_device().unwrap(), sealed);
    assert_eq!(f.store().profile_draft().unwrap(), local);
    assert!(f.store().link_pending().unwrap().is_none());
}

async fn imported_with_newer_private_choices(f: &mut HiddenLink) -> Session {
    let phone = Store::open(&store_path("resume-private-phone"), &f.owner.identity).unwrap();
    phone.set_blocked("bob", true).unwrap();
    phone.set_muted("carol", true).unwrap();
    phone.set_local_name("carol", "original alias").unwrap();
    phone.set_profile_hidden(true).unwrap();
    f.state = phone.link_state().unwrap();
    let store = f.store();
    store.set_link_pending(&f.state).unwrap();
    drop(Session::open_or_create(store, &f.laptop.identity, &f.laptop.device_key, &SEAL).unwrap());
    let mut store = f.store();
    publish::adopt(
        &mut store,
        &mut f.laptop.node,
        &f.laptop.identity,
        &f.state,
        1_000_000,
    )
    .await
    .unwrap();
    store.set_blocked("bob", false).unwrap();
    store.set_muted("carol", false).unwrap();
    store.set_local_name("carol", "later alias").unwrap();
    Session::open(store, &SEAL).unwrap()
}

#[tokio::test]
async fn actual_link_completion_retains_newer_private_choices_after_prefix_import() {
    let mut f = Box::pin(HiddenLink::new()).await;
    let session = Box::pin(imported_with_newer_private_choices(&mut f)).await;
    let certificate = session.certificate().unwrap();
    assert!(Box::pin(resume(&mut f, session)).await.unwrap());
    let store = f.store();
    assert!(!store.blocked("bob").unwrap());
    assert!(!store.muted("carol").unwrap());
    assert_eq!(store.local_name("carol").unwrap(), "later alias");
    assert!(store.profile_hidden().unwrap());
    Box::pin(check_packages_and_retry(&mut f, &certificate)).await;
}

#[tokio::test]
async fn original_existing_session_can_finish_an_unlisted_link() {
    let mut f = Box::pin(HiddenLink::new()).await;
    let store = f.store();
    store.set_link_pending(&f.state).unwrap();
    let session =
        Session::open_or_create(store, &f.laptop.identity, &f.laptop.device_key, &SEAL).unwrap();
    let certificate = session.certificate().unwrap();
    assert!(Box::pin(resume(&mut f, session)).await.unwrap());
    Box::pin(check_packages_and_retry(&mut f, &certificate)).await;
}

#[tokio::test]
async fn reviewed_existing_session_finishes_without_publishing_chosen_private_fields() {
    let mut f = Box::pin(HiddenLink::with_roles(vec![Role::Store, Role::Media])).await;
    let remote = f.prepare_legacy_partial_link().await;
    let store = f.store();
    let local = store.profile_draft().unwrap().unwrap();
    let pending = directory::pending_profile(&store, &local, &f.laptop.identity);
    let session = Session::open(store, &SEAL).unwrap();
    let certificate = session.certificate().unwrap();
    let review = publish::review_adoption(session.store(), &mut f.laptop.node, &f.laptop.identity)
        .await
        .unwrap();
    let choices: Vec<_> = review
        .differences()
        .into_iter()
        .map(|field| (field, publish::AdoptionSource::Local))
        .collect();
    let saved = publish::confirm_adoption_review(
        &session,
        &mut f.laptop.node,
        &f.laptop.identity,
        &review,
        &choices,
    )
    .await
    .unwrap();
    assert!(Box::pin(resume(&mut f, session)).await.unwrap());
    assert_eq!(f.store().profile_draft().unwrap(), Some(saved.draft));
    assert!(f
        .store()
        .profile_photo(&pending.photos[0].thumbnail)
        .unwrap()
        .is_some());
    assert_eq!(
        f.owner
            .node
            .get_profile(&f.owner.identity.peer_id())
            .await
            .unwrap()
            .profile,
        remote
    );
    Box::pin(check_packages_and_retry(&mut f, &certificate)).await;
}

async fn wrong_keys(f: &mut HiddenLink, kind: u8) {
    let wrong = Identity::from_secret(&[9; 32]);
    let session = Session::open(f.store(), &SEAL).unwrap();
    let owner = if kind == 0 {
        &wrong
    } else {
        &f.laptop.identity
    };
    let device = if kind == 1 {
        &wrong
    } else {
        &f.laptop.device_key
    };
    let seal = if kind == 2 { &[9; 32] } else { &SEAL };
    let result = session
        .resume_link(&mut f.laptop.node, owner, device, 2, seal, 1_000_000)
        .await;
    if kind == 2 {
        assert!(matches!(result, Err(dyapp_client::session::Error::Mls(_))));
    } else {
        assert!(matches!(
            result,
            Err(dyapp_client::session::Error::Store(
                dyapp_client::store::Error::WrongIdentity
            ))
        ));
    }
}

#[tokio::test]
async fn original_session_refuses_wrong_recovery_keys_before_importing_link_state() {
    let mut f = Box::pin(HiddenLink::new()).await;
    let remote = f.prepare_legacy_partial_link().await;
    let source = Store::open(&store_path("resume-key-refusal"), &f.owner.identity).unwrap();
    let marker = f.owner.identity.peer_id();
    source.set_blocked(&marker, true).unwrap();
    let state = source.link_state().unwrap();
    let store = f.store();
    store.set_link_pending(&state).unwrap();
    let before = store.profile_draft().unwrap();
    let sealed = store.sealed_device().unwrap();
    drop(store);
    for kind in 0..3 {
        wrong_keys(&mut f, kind).await;
        let store = f.store();
        assert_eq!(store.profile_draft().unwrap(), before);
        assert_eq!(store.sealed_device().unwrap(), sealed);
        assert_eq!(store.link_pending().unwrap(), Some(state.clone()));
        assert!(!store.blocked(&marker).unwrap());
        assert!(store.profile_hidden().unwrap());
        assert!(store.last_resort().unwrap().is_none());
    }
    assert_eq!(
        f.owner
            .node
            .get_profile(&f.owner.identity.peer_id())
            .await
            .unwrap()
            .profile,
        remote
    );
}
