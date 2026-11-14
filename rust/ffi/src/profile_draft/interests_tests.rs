//! Optional native edits must not erase tags from older callers.
use super::*;

fn fixture() -> (String, Arc<IdentityKey>) {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    let root = std::env::temp_dir().join(format!(
        "native-interests-{:032x}",
        u128::from_be_bytes(nonce)
    ));
    std::fs::create_dir(&root).unwrap();
    (root.to_str().unwrap().to_owned(), IdentityKey::generate())
}

#[test]
fn native_interest_save_is_canonical_and_omission_preserves_while_empty_clears() {
    let (dir, owner) = fixture();
    let draft = PublicProfileDraft {
        interests: Some(vec!["  Музыка ".into(), " E\u{301}COLE ".into()]),
        ..Default::default()
    };
    save_profile_draft(dir.clone(), owner.clone(), draft).unwrap();
    let load = || {
        load_profile_draft(dir.clone(), owner.clone())
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        load().interests,
        Some(vec!["музыка".into(), "école".into()])
    );
    save_profile_draft(
        dir.clone(),
        owner.clone(),
        PublicProfileDraft {
            name: "Changed".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        load().interests,
        Some(vec!["музыка".into(), "école".into()])
    );
    save_profile_draft(
        dir.clone(),
        owner.clone(),
        PublicProfileDraft {
            interests: Some(Vec::new()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(load().interests, Some(Vec::new()));
}

#[test]
fn optional_interest_projection_preserves_unmentioned_tags_and_version() {
    let mut current = Profile {
        interests: vec!["old".into()],
        version: 9,
        ..Default::default()
    };
    PublicProfileDraft::default().apply_to(&mut current);
    assert_eq!(current.interests, ["old"]);
    PublicProfileDraft {
        interests: Some(vec!["  MUSIC  ".into()]),
        ..Default::default()
    }
    .apply_to(&mut current);
    assert_eq!(current.interests, ["music"]);
    assert_eq!(current.version, 9);
}

#[test]
fn rejected_native_interest_edits_leave_all_stored_fields_unchanged() {
    let (dir, owner) = fixture();
    save_profile_draft(
        dir.clone(),
        owner.clone(),
        PublicProfileDraft {
            name: "Original".into(),
            interests: Some(vec!["music".into()]),
            ..Default::default()
        },
    )
    .unwrap();
    let before = load_profile_draft(dir.clone(), owner.clone()).unwrap();
    for interests in [
        vec!["music".into(), " ".into()],
        vec!["mu\u{202E}sic".into()],
        vec!["x".repeat(dyapp_profile::MAX_FIELD_CHARS + 1)],
        vec!["music".into(); dyapp_profile::MAX_INTERESTS + 1],
    ] {
        let result = save_profile_draft(
            dir.clone(),
            owner.clone(),
            PublicProfileDraft {
                name: "Must not replace".into(),
                interests: Some(interests),
                ..Default::default()
            },
        );
        assert!(matches!(result, Err(CoreError::Failed(code)) if code == "invalidInterests"));
        assert_eq!(
            load_profile_draft(dir.clone(), owner.clone()).unwrap(),
            before
        );
    }
}
