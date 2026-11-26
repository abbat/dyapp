use super::*;
use std::fs;

fn temp_root(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "dyapp-desktop-link-review-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn projection_retains_complete_editable_profile_and_photo_references() {
    let mut profile = dyapp_profile::Profile {
        version: 7,
        income_from: Some(10),
        income_to: Some(u64::MAX),
        has_kids: Some(false),
        wants_more_kids: Some(true),
        interests: vec!["rust".into()],
        photos: vec![dyapp_profile::Photo {
            thumbnail: vec![1; 32],
            full: vec![vec![2; 32], vec![3; 32]],
        }],
        education_level: 4,
        profession: "Engineer".into(),
        company: "Example".into(),
        industry: "Software".into(),
        career_stage: "Senior".into(),
        work_life_balance: "Remote".into(),
        name: "Owner".into(),
        ..Default::default()
    };
    profile.seeking_age_to = Some(99);
    let projected = ReviewProfile::from(&profile);
    assert_eq!(projected.version, "7");
    assert_eq!(projected.fields.name, "Owner");
    assert_eq!(projected.fields.seeking_age_to, Some(99));
    assert_eq!(
        projected.income_to.as_deref(),
        Some(u64::MAX.to_string().as_str())
    );
    assert_eq!(projected.interests, ["rust"]);
    assert_eq!(projected.photos[0].full, [vec![2; 32], vec![3; 32]]);
    assert_eq!(projected.profession, "Engineer");
    assert_eq!(projected.work_life_balance, "Remote");
}

#[test]
fn admission_never_generates_missing_device_or_sealing_secrets() {
    let root = temp_root("existing-only");
    let (slot, owner, _) = slots::create(&root, "file").unwrap();
    assert!(admit(&root, &slot.dir, &owner).is_err());
    assert!(!slot.device_path().exists());
    assert!(!slot.seal_path().exists());

    let device = Identity::try_generate().unwrap();
    crate::save_key(&slot, &slot.device_path(), &slot.device_name(), &device).unwrap();
    Store::open(&slot.dir.join("client.db"), &owner).unwrap();
    assert!(admit(&root, &slot.dir, &owner).is_err());
    assert!(slot.device_path().is_file());
    assert!(!slot.seal_path().exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn policy_errors_remain_distinct_from_backend_diagnostics() {
    assert_eq!(
        publish_error(publish::Failure::AdoptionReviewRequired),
        REVIEW_REQUIRED
    );
    assert_eq!(
        store_error(store::Error::LinkStateReviewRequired),
        "privateStateReviewRequired"
    );
    assert_eq!(
        store_error(store::Error::LinkEnrollmentReviewRequired),
        "enrollmentAuthorizationRequired"
    );
    assert_eq!(session_error(session::Error::Revoked), "revoked");
    assert_eq!(
        confirm_error(AdoptionConfirmError::DeviceNotListed),
        "deviceNotListed"
    );
    assert_eq!(
        photo_error(publish::AdoptionPhotoError::Unavailable),
        "photoUnavailable"
    );
}
