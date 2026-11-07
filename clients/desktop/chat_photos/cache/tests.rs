use super::*;
use dyapp_identity::Identity;
use std::{fs, path::Path, sync::Arc};

#[tokio::test]
async fn cache_commands_are_numeric_local_and_reject_stale_or_closed_identity() {
    let owner = Arc::new(Identity::generate());
    let peer = owner.peer_id();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/desktop-media-cache")
        .join(&peer);
    fs::create_dir_all(&dir).unwrap();
    let state = IdentityState::new();
    assert!(run(&state, peer.clone(), Operation::Read).await.is_err());
    state
        .start_transition()
        .unwrap()
        .commit(Some((owner, dir.clone())))
        .unwrap();
    assert!(run(&state, "stale".into(), Operation::Clear).await.is_err());
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    let initial = run(&state, peer.clone(), Operation::Read).await.unwrap();
    assert!(initial.enabled);
    assert_eq!((initial.limit_bytes, initial.used_bytes), (268_435_456, 0));
    let custom = MediaCacheConfig {
        enabled: false,
        limit_bytes: 393_216_123,
    };
    let saved = run(&state, peer.clone(), Operation::Set(custom))
        .await
        .unwrap();
    assert!(!saved.enabled);
    assert_eq!(saved.limit_bytes, custom.limit_bytes);
    assert!(run(
        &state,
        peer.clone(),
        Operation::Set(MediaCacheConfig {
            enabled: true,
            limit_bytes: 0
        })
    )
    .await
    .is_err());
    let originals = dir.join("originals");
    fs::create_dir(&originals).unwrap();
    fs::write(originals.join("download"), b"cached").unwrap();
    let used = run(&state, peer.clone(), Operation::Read).await.unwrap();
    assert_eq!(used.used_bytes, 6);
    let cleared = run(&state, peer.clone(), Operation::Clear).await.unwrap();
    assert_eq!(cleared.used_bytes, 0);
    assert_eq!(cleared.limit_bytes, custom.limit_bytes);
    assert!(!cleared.enabled);
    let transition = state.start_transition().unwrap();
    assert!(run(&state, peer.clone(), Operation::Read).await.is_err());
    transition.commit(None).unwrap();
    assert!(run(&state, peer, Operation::Clear).await.is_err());
}
