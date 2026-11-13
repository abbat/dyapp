use super::*;
use crate::store::tests::temp_db_path;
use dyapp_profile::Photo;

fn frozen(local: &Profile, remote: Profile) -> (AdoptionReview, Store) {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store.save_profile_draft(local).unwrap();
    store.set_profile_hidden(true).unwrap();
    store.set_link_pending(b"fixture link").unwrap();
    let context = store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    (AdoptionReview { context, remote }, store)
}

#[test]
fn explicit_sources_form_a_preview_without_changing_either_snapshot_or_storage() {
    let local = Profile {
        version: 9,
        name: "local".into(),
        about: "local about".into(),
        height: Some(180),
        ..Default::default()
    };
    let remote = Profile {
        version: 8,
        name: "remote".into(),
        about: "remote about".into(),
        height: Some(170),
        ..Default::default()
    };
    let (review, store) = frozen(&local, remote.clone());
    assert_eq!(
        review.differences(),
        vec![
            AdoptionField::Name,
            AdoptionField::Height,
            AdoptionField::About
        ]
    );
    let preview = review
        .select_fields(&[
            (AdoptionField::Name, AdoptionSource::Local),
            (AdoptionField::About, AdoptionSource::Remote),
            (AdoptionField::Height, AdoptionSource::Local),
        ])
        .unwrap();
    assert_eq!(preview.name, local.name);
    assert_eq!(preview.about, remote.about);
    assert_eq!(preview.height, local.height);
    assert_eq!(preview.version, remote.version);
    assert_eq!(review.local_profile(), &local);
    assert_eq!(review.remote_profile(), &remote);
    assert!(review.hidden());
    assert_eq!(store.profile_draft().unwrap(), Some(local));
}

#[test]
fn incomplete_duplicate_and_unoffered_decisions_are_rejected_without_defaults() {
    let local = Profile {
        name: "local".into(),
        ..Default::default()
    };
    let remote = Profile {
        name: "remote".into(),
        ..Default::default()
    };
    let (review, store) = frozen(&local, remote.clone());
    assert_eq!(
        review.select_fields(&[]),
        Err(AdoptionSelectionError::Incomplete)
    );
    assert_eq!(
        review.select_fields(&[
            (AdoptionField::Name, AdoptionSource::Local),
            (AdoptionField::Name, AdoptionSource::Remote),
        ]),
        Err(AdoptionSelectionError::Duplicate)
    );
    assert_eq!(
        review.select_fields(&[(AdoptionField::Height, AdoptionSource::Local)]),
        Err(AdoptionSelectionError::UnexpectedField)
    );
    assert_eq!(
        review
            .select_fields(&[(AdoptionField::Name, AdoptionSource::Remote)])
            .unwrap(),
        remote
    );
    assert_eq!(store.profile_draft().unwrap(), Some(local));
}

#[test]
fn dates_location_income_and_seeking_are_selected_as_complete_groups() {
    let local = Profile {
        country: "local country".into(),
        place: "local place".into(),
        income_from: Some(10),
        income_to: Some(20),
        birth_year: Some(2000),
        birth_month: Some(1),
        birth_day: Some(2),
        seeking_genders: vec![1],
        seeking_age_from: Some(18),
        seeking_age_to: Some(22),
        ..Default::default()
    };
    let remote = Profile {
        country: "remote country".into(),
        place: "remote place".into(),
        income_from: Some(30),
        income_to: Some(40),
        birth_year: Some(1990),
        birth_month: Some(3),
        birth_day: Some(4),
        seeking_genders: vec![2],
        seeking_age_from: Some(25),
        seeking_age_to: Some(35),
        ..Default::default()
    };
    let (review, _) = frozen(&local, remote);
    assert_eq!(
        review.differences(),
        vec![
            AdoptionField::Location,
            AdoptionField::Income,
            AdoptionField::Birth,
            AdoptionField::Seeking
        ]
    );
    let choices: Vec<_> = review
        .differences()
        .into_iter()
        .map(|field| (field, AdoptionSource::Local))
        .collect();
    let preview = review.select_fields(&choices).unwrap();
    assert_eq!(preview, local);
}

#[test]
fn local_photos_and_interests_never_restore_local_device_authorization() {
    let owner = Identity::from_secret(&[7; 32]);
    let local_key = Identity::from_secret(&[8; 32]);
    let remote_key = Identity::from_secret(&[9; 32]);
    let local_device =
        dyapp_messaging::mls::Device::new(&owner, local_key.public_key(), 1).unwrap();
    let remote_device =
        dyapp_messaging::mls::Device::new(&owner, remote_key.public_key(), 1).unwrap();
    let local = Profile {
        devices: vec![local_device.certificate().clone()],
        device_owners: vec![crate::owner_binding(&local_key, &owner.public_key())],
        photos: vec![Photo {
            full: vec![vec![1; 32]],
            thumbnail: vec![2; 32],
        }],
        interests: vec!["local".into()],
        pow_bits: 2,
        pow_nonce: dyapp_profile::solve(&owner.public_key(), 2, 0, 1024).unwrap(),
        ..Default::default()
    };
    let remote = Profile {
        devices: vec![remote_device.certificate().clone()],
        device_owners: vec![crate::owner_binding(&remote_key, &owner.public_key())],
        interests: vec!["remote".into()],
        ..Default::default()
    };
    let (review, _) = frozen(&local, remote.clone());
    assert_eq!(
        review.differences(),
        vec![AdoptionField::Interests, AdoptionField::Photos]
    );
    let preview = review
        .select_fields(&[
            (AdoptionField::Photos, AdoptionSource::Local),
            (AdoptionField::Interests, AdoptionSource::Local),
        ])
        .unwrap();
    assert_eq!(preview.devices, remote.devices);
    assert_eq!(preview.device_owners, remote.device_owners);
    assert_eq!(preview.photos, local.photos);
    assert_eq!(preview.interests, local.interests);
    assert_eq!(
        (preview.pow_bits, preview.pow_nonce),
        (local.pow_bits, local.pow_nonce)
    );
    assert_eq!(review.remote_profile(), &remote);
}
