use super::*;
use dyapp_identity::Identity;

fn store() -> (Store, std::path::PathBuf, Identity) {
    let owner = Identity::generate();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/test-media-cache")
        .join(owner.peer_id());
    fs::create_dir_all(&dir).unwrap();
    (
        Store::open(&dir.join("state.sqlite"), &owner).unwrap(),
        dir,
        owner,
    )
}

#[test]
fn numeric_budget_survives_restart_disabled_state_and_invalid_writes() {
    let (store, dir, owner) = store();
    assert_eq!(
        store.media_cache_config().unwrap(),
        MediaCacheConfig::default()
    );
    assert_eq!(store.kept_originals_mib().unwrap(), 256);
    for limit in [1_048_576, 393_216_123, 10_737_418_240] {
        let config = MediaCacheConfig {
            enabled: true,
            limit_bytes: limit,
        };
        store.set_media_cache_config(config).unwrap();
        assert_eq!(store.media_cache_config().unwrap(), config);
        store.set_kept_originals_mib(0).unwrap();
        assert_eq!(store.kept_originals_mib().unwrap(), 0);
        assert_eq!(
            store.media_cache_config().unwrap(),
            MediaCacheConfig {
                enabled: false,
                ..config
            }
        );
    }
    let before = store.media_cache_config().unwrap();
    for limit in [0, 1_048_575, 10_737_418_241, u64::MAX] {
        assert!(store
            .set_media_cache_config(MediaCacheConfig {
                enabled: false,
                limit_bytes: limit
            })
            .is_err());
        assert_eq!(store.media_cache_config().unwrap(), before);
    }
    drop(store);
    let store = Store::open(&dir.join("state.sqlite"), &owner).unwrap();
    assert_eq!(store.media_cache_config().unwrap(), before);
    store
        .set_media_cache_config(MediaCacheConfig {
            enabled: true,
            ..before
        })
        .unwrap();
    assert_eq!(store.kept_originals_mib().unwrap(), 10_240);
    store.set_kept_originals_mib(384).unwrap();
    assert_eq!(store.kept_originals_mib().unwrap(), 384);
}

#[test]
fn corrupt_or_unsupported_setting_cannot_change_retention() {
    let (store, _, _) = store();
    store
        .set_media_cache_config(MediaCacheConfig::default())
        .unwrap();
    let valid: Vec<u8> = store
        .db
        .query_row(
            "SELECT `value` FROM `settings` WHERE `key` = ?1",
            [SETTING],
            |row| row.get(0),
        )
        .unwrap();
    let mut bad_flag = valid.clone();
    bad_flag[0] = 2;
    for (version, value) in [(1, vec![1u8]), (2, valid), (1, bad_flag), (1, vec![0; 9])] {
        store
            .db
            .execute(
                "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, ?2, ?3)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = ?2, `value` = ?3",
                params![SETTING, version, value],
            )
            .unwrap();
        assert!(store.media_cache_config().is_err());
        assert!(store.kept_originals_mib().is_err());
    }
}

#[cfg(unix)]
#[test]
fn clear_refuses_a_redirected_cache_folder() {
    let (store, dir, _) = store();
    let outside = dir.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("valuable"), b"preserve").unwrap();
    std::os::unix::fs::symlink(&outside, dir.join(FOLDER)).unwrap();
    assert!(store.media_cache_usage(&dir).is_err());
    assert!(store.clear_media_cache(&dir).is_err());
    assert_eq!(fs::read(outside.join("valuable")).unwrap(), b"preserve");
}

#[test]
fn usage_and_explicit_clear_are_local_and_preserve_preference() {
    let (store, dir, _) = store();
    assert_eq!(store.media_cache_usage(&dir).unwrap(), 0);
    store.clear_media_cache(&dir).unwrap();
    let cache = dir.join(FOLDER);
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join("a"), [0; 17]).unwrap();
    fs::write(cache.join("b"), [1; 29]).unwrap();
    fs::write(dir.join("unrelated"), b"saved data").unwrap();
    let config = MediaCacheConfig {
        enabled: false,
        limit_bytes: 123_456_789,
    };
    store.set_media_cache_config(config).unwrap();
    assert_eq!(store.media_cache_usage(&dir).unwrap(), 46);
    store.clear_media_cache(&dir).unwrap();
    assert_eq!(store.media_cache_usage(&dir).unwrap(), 0);
    assert_eq!(store.media_cache_config().unwrap(), config);
    assert_eq!(fs::read(dir.join("unrelated")).unwrap(), b"saved data");
    fs::write(cache.join("a"), b"keep").unwrap();
    fs::create_dir(cache.join("unexpected")).unwrap();
    assert!(store.media_cache_usage(&dir).is_err());
    assert!(store.clear_media_cache(&dir).is_err());
    assert_eq!(fs::read(cache.join("a")).unwrap(), b"keep");
}
