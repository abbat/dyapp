//! Real offline native projections of common cache input and confirmation-before-mutation.
use super::favorites::Fixture;
use super::*;

#[test]
fn native_cache_edit_validates_exact_input_and_preserves_independent_fields() {
    let f = Fixture::new();
    let initial = f.chats.media_cache_settings().unwrap();
    assert_eq!(initial.limit_mib, "256");
    assert_eq!(
        (initial.min_mib.as_str(), initial.max_mib.as_str()),
        ("1", "10240")
    );
    let edit = ChatMediaCacheEdit::Budget { mib: "1.5".into() };
    let pending = f.chats.change_media_cache(edit.clone(), false).unwrap();
    assert!(pending.confirmation_required);
    assert_eq!(pending.settings, initial);
    let applied = f.chats.change_media_cache(edit, true).unwrap();
    assert!(!applied.confirmation_required);
    assert_eq!(applied.settings.limit_mib, "1.5");
    assert_eq!(applied.settings.state.limit_bytes, 1_572_864);
    assert!(matches!(
        f.chats.change_media_cache(
            ChatMediaCacheEdit::Budget {
                mib: "1.0000001".into()
            },
            true
        ),
        Err(ChatMediaCacheError::InvalidLimit)
    ));
    assert_eq!(f.chats.media_cache_settings().unwrap(), applied.settings);
    let off = f
        .chats
        .change_media_cache(ChatMediaCacheEdit::Retention { enabled: false }, true)
        .unwrap();
    assert!(!off.settings.state.enabled);
    assert_eq!(off.settings.state.limit_bytes, 1_572_864);
    let reopened = f.reopen();
    assert_eq!(reopened.chats.media_cache_settings().unwrap(), off.settings);
}

#[test]
fn native_clear_refuses_unconfirmed_mutation_and_preserves_other_identity() {
    let f = Fixture::new();
    let other = Fixture::new();
    let folder = std::path::Path::new(&f.dir).join("originals");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("cached"), b"download").unwrap();
    let before = f.chats.media_cache_settings().unwrap();
    let pending = f
        .chats
        .change_media_cache(ChatMediaCacheEdit::Clear, false)
        .unwrap();
    assert!(pending.confirmation_required);
    assert_eq!(pending.settings, before);
    assert_eq!(std::fs::read(folder.join("cached")).unwrap(), b"download");
    let cleared = f
        .chats
        .change_media_cache(ChatMediaCacheEdit::Clear, true)
        .unwrap();
    assert!(!cleared.confirmation_required);
    assert_eq!(cleared.settings.state.used_bytes, 0);
    assert_eq!(cleared.settings.state.limit_bytes, before.state.limit_bytes);
    assert_eq!(other.chats.media_cache_settings().unwrap().limit_mib, "256");
    assert_eq!(f.chats.chat_rows().unwrap().len(), 0);
}
