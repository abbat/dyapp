use super::*;

#[test]
fn replacing_and_retiring_handles_cannot_name_a_new_opening() {
    let state = ViewState::default();
    let context = Arc::new(Mutex::new(Context::default()));
    let owner = Identity::try_generate().unwrap();
    let peer = Identity::try_generate().unwrap().peer_id();
    let dir = Path::new("original");
    let old = state
        .prepare(Arc::clone(&context), &owner, dir, peer.clone())
        .unwrap();
    let original = state.capture(&old, dir).unwrap();
    let next = state.prepare(context, &owner, dir, peer.clone()).unwrap();
    assert_ne!(old, next);
    assert!(state.capture(&old, dir).is_err());
    assert_eq!(original.opening.peer(), peer);
    state.retire(&old).unwrap();
    assert!(state.capture(&next, dir).is_ok());
    assert!(state.capture(&next, Path::new("replacement")).is_err());
    state.retire(&next).unwrap();
    assert!(state.capture(&next, dir).is_err());
}

#[test]
fn exhausted_handles_refuse_without_replacing_the_current_capture() {
    let state = ViewState::default();
    let context = Arc::new(Mutex::new(Context::default()));
    let owner = Identity::try_generate().unwrap();
    let peer = Identity::try_generate().unwrap().peer_id();
    let dir = Path::new("original");
    let token = state
        .prepare(Arc::clone(&context), &owner, dir, peer.clone())
        .unwrap();
    state.0.lock().unwrap().next = u64::MAX;
    assert!(state.prepare(context, &owner, dir, peer).is_err());
    assert!(state.capture(&token, dir).is_ok());
}

#[tokio::test]
async fn disabled_and_retired_views_stop_before_network_or_device_creation() {
    let state = ViewState::default();
    let context = Arc::new(Mutex::new(Context::default()));
    let owner = Identity::try_generate().unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build")
        .join(format!("desktop-profile-view-{}", owner.peer_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let peer = Identity::try_generate().unwrap().peer_id();
    let token = state
        .prepare(Arc::clone(&context), &owner, &dir, peer.clone())
        .unwrap();
    let capture = state.capture(&token, &dir).unwrap();
    assert!(Box::pin(send(&dir, &dir, &owner, &capture))
        .await
        .unwrap()
        .is_none());
    let store = super::super::open(&dir, &owner).unwrap();
    store.set_send_profile_views(true).unwrap();
    assert!(Box::pin(send(&dir, &dir, &owner, &capture))
        .await
        .unwrap()
        .is_none());
    let token = state
        .prepare(Arc::clone(&context), &owner, &dir, peer.clone())
        .unwrap();
    let capture = state.capture(&token, &dir).unwrap();
    capture.opening.retire();
    assert!(Box::pin(send(&dir, &dir, &owner, &capture))
        .await
        .unwrap()
        .is_none());
    let token = state.prepare(context, &owner, &dir, peer).unwrap();
    let capture = state.capture(&token, &dir).unwrap();
    assert_eq!(
        Box::pin(send(&dir, &dir, &owner, &capture)).await.err(),
        Some("notConnected".into())
    );
    assert!(store.sealed_device().unwrap().is_none());
    assert_eq!(store.known_groups().unwrap().len(), 0);
    assert!(store.due_outbound(i64::MAX).unwrap().is_empty());
}
