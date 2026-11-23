use super::*;
use dyapp_identity::Identity;
use std::{fs, path::Path, sync::Arc};

async fn request(
    state: &IdentityState,
    sessions: &chats::Sessions,
    peer: &str,
    operation: Operation,
) -> CacheView {
    run(state, sessions, peer.into(), operation).await.unwrap()
}

async fn check_budget_round_trip(
    state: &IdentityState,
    sessions: &chats::Sessions,
    peer: &str,
) -> MediaCacheConfig {
    let custom = MediaCacheConfig {
        enabled: false,
        limit_bytes: 393_216_123,
    };
    let saved = request(state, sessions, peer, Operation::Set(custom)).await;
    assert!(!saved.enabled);
    assert_eq!(saved.limit_bytes, custom.limit_bytes);
    assert_eq!(input::parse_mib(&saved.limit_mib), Some(custom.limit_bytes));
    assert_eq!(input::parse_mib(&saved.min_mib), Some(saved.min_bytes));
    assert_eq!(input::parse_mib(&saved.max_mib), Some(saved.max_bytes));
    assert!(run(
        state,
        sessions,
        peer.into(),
        Operation::Set(MediaCacheConfig {
            enabled: true,
            limit_bytes: 0,
        })
    )
    .await
    .is_err());
    custom
}

async fn check_confirmed_clear(
    state: &IdentityState,
    sessions: &chats::Sessions,
    peer: &str,
    dir: &Path,
    remembered: u64,
) {
    let originals = dir.join("originals");
    fs::create_dir(&originals).unwrap();
    fs::write(originals.join("download"), b"cached").unwrap();
    assert_eq!(
        request(state, sessions, peer, Operation::Read)
            .await
            .used_bytes,
        6
    );
    let pending = request(
        state,
        sessions,
        peer,
        Operation::Change(MediaCacheChange::Clear, false),
    )
    .await;
    assert!(pending.confirmation_required);
    assert_eq!(pending.used_bytes, 6);
    assert_eq!(fs::read(originals.join("download")).unwrap(), b"cached");
    let cleared = request(
        state,
        sessions,
        peer,
        Operation::Change(MediaCacheChange::Clear, true),
    )
    .await;
    assert_eq!(cleared.used_bytes, 0);
    assert_eq!(cleared.limit_bytes, remembered);
    assert!(!cleared.enabled);
}

async fn check_targeted_retention(
    state: &IdentityState,
    sessions: &chats::Sessions,
    peer: &str,
    remembered: u64,
) {
    let enabled = request(
        state,
        sessions,
        peer,
        Operation::Change(MediaCacheChange::Retention(true), false),
    )
    .await;
    assert!(enabled.enabled);
    assert_eq!(enabled.limit_bytes, remembered);
    let edit: Change =
        serde_json::from_value(serde_json::json!({"kind": "budget", "mib": "128"})).unwrap();
    let refused = request(state, sessions, peer, Operation::Change(edit.into(), false)).await;
    assert!(refused.confirmation_required);
    let unchanged = request(state, sessions, peer, Operation::Read).await;
    assert!(unchanged.enabled);
    assert_eq!(unchanged.limit_bytes, remembered);
    let disabled = request(
        state,
        sessions,
        peer,
        Operation::Change(MediaCacheChange::Retention(false), true),
    )
    .await;
    assert!(!disabled.enabled);
    assert_eq!(disabled.limit_bytes, remembered);
}

async fn check_targeted_budget(state: &IdentityState, sessions: &chats::Sessions, peer: &str) {
    let reduced = request(
        state,
        sessions,
        peer,
        Operation::Change(MediaCacheChange::Budget("128".into()), false),
    )
    .await;
    assert!(!reduced.enabled);
    assert!(!reduced.confirmation_required);
    assert_eq!(reduced.limit_bytes, 128 * 1024 * 1024);
    assert_eq!(
        run(
            state,
            sessions,
            peer.into(),
            Operation::Change(MediaCacheChange::Budget("0".into()), false)
        )
        .await
        .err(),
        Some("invalidLimit".into())
    );
    let after = request(state, sessions, peer, Operation::Read).await;
    assert_eq!(after.limit_bytes, reduced.limit_bytes);
    assert!(!after.enabled);
    assert!(
        serde_json::from_value::<Change>(serde_json::json!({"kind": "budget", "mib": 128}))
            .is_err()
    );
}

#[tokio::test]
async fn cache_commands_are_numeric_local_and_reject_stale_or_closed_identity() {
    let owner = Arc::new(Identity::try_generate().unwrap());
    let peer = owner.peer_id();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/desktop-media-cache")
        .join(&peer);
    fs::create_dir_all(&dir).unwrap();
    let state = IdentityState::new();
    let sessions = chats::Sessions::default();
    assert!(run(&state, &sessions, peer.clone(), Operation::Read)
        .await
        .is_err());
    state
        .start_transition()
        .unwrap()
        .commit(Some((owner, dir.clone())))
        .unwrap();
    assert!(run(&state, &sessions, "stale".into(), Operation::Clear)
        .await
        .is_err());
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    let initial = request(&state, &sessions, &peer, Operation::Read).await;
    assert!(initial.enabled);
    assert_eq!((initial.limit_bytes, initial.used_bytes), (268_435_456, 0));
    let custom = check_budget_round_trip(&state, &sessions, &peer).await;
    check_confirmed_clear(&state, &sessions, &peer, &dir, custom.limit_bytes).await;
    check_targeted_retention(&state, &sessions, &peer, custom.limit_bytes).await;
    check_targeted_budget(&state, &sessions, &peer).await;
    let transition = state.start_transition().unwrap();
    assert!(run(&state, &sessions, peer.clone(), Operation::Read)
        .await
        .is_err());
    transition.commit(None).unwrap();
    assert!(run(&state, &sessions, peer, Operation::Clear)
        .await
        .is_err());
}
