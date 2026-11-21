use super::*;
use dyapp_identity::Identity;
use std::fs;

fn fixture() -> (Store, std::path::PathBuf) {
    let owner = Identity::try_generate().unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/test-media-cache-control")
        .join(owner.peer_id());
    fs::create_dir_all(dir.join("originals")).unwrap();
    (Store::open(&dir.join("state.sqlite"), &owner).unwrap(), dir)
}

#[test]
fn confirmation_rechecks_current_budget_and_leaves_files_untouched_until_accepted() {
    let (store, dir) = fixture();
    fs::write(dir.join("originals/a"), b"original").unwrap();
    let before = store.media_cache_snapshot(&dir).unwrap();
    let change = MediaCacheChange::Budget("128".into());
    let pending = store.change_media_cache(&dir, &change, false).unwrap();
    assert!(pending.confirmation_required);
    assert_eq!(pending.state, before);
    assert_eq!(fs::read(dir.join("originals/a")).unwrap(), b"original");
    // Another completed edit makes this an increase; use actual state, not the old 256 MiB view.
    store
        .set_media_cache_config(MediaCacheConfig {
            enabled: true,
            limit_bytes: 64 * 1_048_576,
        })
        .unwrap();
    let applied = store.change_media_cache(&dir, &change, false).unwrap();
    assert!(!applied.confirmation_required);
    assert!(applied.state.config.enabled);
    assert_eq!(applied.state.config.limit_bytes, 128 * 1_048_576);
    assert_eq!(applied.state.used_bytes, 0);
}

#[test]
fn retention_preserves_actual_budget_and_clear_preserves_settings_and_unrelated_data() {
    let (store, dir) = fixture();
    let budget = MediaCacheChange::Budget("1.5".into());
    store.change_media_cache(&dir, &budget, true).unwrap();
    let off = MediaCacheChange::Retention(false);
    assert!(
        store
            .change_media_cache(&dir, &off, false)
            .unwrap()
            .confirmation_required
    );
    let state = store.change_media_cache(&dir, &off, true).unwrap().state;
    assert_eq!(state.config.limit_bytes, 1_572_864);
    assert!(!state.config.enabled);
    fs::write(dir.join("originals/a"), b"new").unwrap();
    fs::write(dir.join("unrelated"), b"saved pack/history stand-in").unwrap();
    let pending = store
        .change_media_cache(&dir, &MediaCacheChange::Clear, false)
        .unwrap();
    assert!(pending.confirmation_required);
    assert_eq!(pending.state.used_bytes, 3);
    let cleared = store
        .change_media_cache(&dir, &MediaCacheChange::Clear, true)
        .unwrap();
    assert_eq!(cleared.state.config, state.config);
    assert_eq!(cleared.state.used_bytes, 0);
    assert_eq!(
        fs::read(dir.join("unrelated")).unwrap(),
        b"saved pack/history stand-in"
    );
}

#[test]
fn invalid_budget_and_unknown_directory_do_not_mutate_the_setting_or_files() {
    let (store, dir) = fixture();
    let before = store.media_cache_snapshot(&dir).unwrap();
    let invalid = MediaCacheChange::Budget("1.0000001".into());
    assert!(matches!(
        store.change_media_cache(&dir, &invalid, true),
        Err(MediaCacheChangeError::InvalidLimit)
    ));
    assert_eq!(store.media_cache_snapshot(&dir).unwrap(), before);
    fs::write(dir.join("originals/a"), b"keep").unwrap();
    fs::create_dir(dir.join("originals/unexpected")).unwrap();
    assert!(store
        .change_media_cache(&dir, &MediaCacheChange::Retention(false), true)
        .is_err());
    assert_eq!(store.media_cache_config().unwrap(), before.config);
    assert_eq!(fs::read(dir.join("originals/a")).unwrap(), b"keep");
}
