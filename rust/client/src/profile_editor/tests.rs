use super::*;
use dyapp_identity::SignedRecord;
use dyapp_profile::Photo;

#[test]
fn validation_preserves_error_order_and_messages() {
    let mut draft = Profile {
        name: "\u{200b}".into(),
        height: Some(1),
        about: "\u{200b}".into(),
        seeking_age_from: Some(17),
        gender: u32::MAX,
        birth_year: Some(1900),
        birth_month: Some(2),
        birth_day: Some(29),
        country: "FR".into(),
        place: "2950159".into(),
        ..Profile::default()
    };
    assert_eq!(validate(&draft).unwrap_err().to_string(), "invalidName");
    draft.name = "  Owner  ".into();
    assert_eq!(validate(&draft).unwrap_err().to_string(), "invalidHeight");
    draft.height = Some(180);
    assert_eq!(validate(&draft).unwrap_err().to_string(), "invalidAbout");
    draft.about = "  About me  ".into();
    assert_eq!(validate(&draft).unwrap_err().to_string(), "invalidSeeking");
    draft.seeking_age_from = Some(18);
    assert_eq!(
        validate(&draft).unwrap_err().to_string(),
        "invalid profile scale"
    );
    draft.gender = 1;
    assert_eq!(
        validate(&draft).unwrap_err().to_string(),
        "invalid birth date"
    );
    draft.birth_year = None;
    assert_eq!(validate(&draft).unwrap_err().to_string(), "invalid place");
    draft.country = "DE".into();
    assert_eq!(validate(&draft), Ok(()));
}

#[test]
fn node_verification_accepts_future_codes_that_the_editor_does_not_offer() {
    let mut draft = Profile {
        version: 1,
        gender: u32::MAX,
        seeking_genders: vec![u32::MAX],
        ..Profile::default()
    };
    let owner = dyapp_identity::Identity::generate();
    assert!(profile::verify(&draft.sign(&owner)).is_ok());
    assert_eq!(validate(&draft), Err(DraftError::Seeking));
    draft.seeking_genders.clear();
    assert!(profile::verify(&draft.sign(&owner)).is_ok());
    assert_eq!(validate(&draft), Err(DraftError::Scale));
}

#[test]
fn categorical_editor_choices_are_closed_and_partial_dates_are_valid() {
    assert_eq!(validate(&Profile::default()), Ok(()));
    let mut draft = Profile {
        career_ambition: 15,
        ..Profile::default()
    };
    assert_eq!(validate(&draft), Err(DraftError::Scale));
    draft.career_ambition = 0;
    draft.seeking_genders = vec![u32::MAX];
    assert_eq!(validate(&draft), Err(DraftError::Seeking));
    draft.seeking_genders = vec![1];
    draft.birth_day = Some(29);
    draft.birth_month = Some(2);
    assert_eq!(validate(&draft), Ok(()));
}

#[test]
fn normalized_application_preserves_the_entire_noneditable_profile() {
    let mut target = Profile {
        version: 7,
        pow_nonce: 123,
        pow_bits: 4,
        photos: vec![Photo::default()],
        devices: vec![SignedRecord::default()],
        device_owners: vec![SignedRecord::default()],
        education_level: 2,
        income_from: Some(100),
        has_kids: Some(true),
        interests: vec!["music".into()],
        seeking_genders: vec![1],
        seeking_age_from: Some(18),
        seeking_age_to: Some(50),
        ..Profile::default()
    };
    let expected = Profile {
        name: "Owner".into(),
        about: "About".into(),
        gender: 2,
        not_looking: true,
        seeking_genders: Vec::new(),
        seeking_age_from: None,
        seeking_age_to: None,
        ..target.clone()
    };
    let input = Profile {
        name: "  Owner  ".into(),
        about: "  About  ".into(),
        gender: 2,
        not_looking: true,
        version: 99,
        pow_nonce: 999,
        pow_bits: 9,
        education_level: 99,
        seeking_genders: vec![1],
        seeking_age_from: Some(18),
        seeking_age_to: Some(50),
        ..Profile::default()
    };
    assert_eq!(validate(&input), Ok(()));
    apply(&input, &mut target);
    assert_eq!(target, expected);
}

#[test]
fn all_editable_fields_reach_the_projection() {
    let input = Profile {
        name: " Owner ".into(),
        about: " About ".into(),
        gender: 1,
        orientation: 2,
        relationship_goal: 3,
        role_preference: 4,
        career_ambition: 10,
        financial_philosophy: 20,
        employment_status: 30,
        fitness_level: 40,
        zodiac: 5,
        birth_year: Some(2000),
        birth_month: Some(2),
        birth_day: Some(29),
        country: "DE".into(),
        place: "2950159".into(),
        height: Some(180),
        smoking: 1,
        drinking: 2,
        tattoos: 3,
        marital_status: 4,
        pets: 5,
        seeking_genders: vec![1, 2],
        seeking_age_from: Some(18),
        seeking_age_to: Some(50),
        ..Profile::default()
    };
    let expected = Profile {
        name: "Owner".into(),
        about: "About".into(),
        ..input.clone()
    };
    let mut target = Profile::default();
    apply(&input, &mut target);
    assert_eq!(target, expected);
}

#[test]
fn read_only_application_does_not_add_validation_or_clear_active_seeking() {
    let input = Profile {
        name: "  Owner  ".into(),
        seeking_genders: vec![u32::MAX],
        seeking_age_from: Some(17),
        seeking_age_to: Some(20),
        ..Profile::default()
    };
    assert_eq!(validate(&input), Err(DraftError::Seeking));
    let mut target = Profile::default();
    apply(&input, &mut target);
    assert_eq!(target.name, "Owner");
    assert_eq!(target.seeking_genders, input.seeking_genders);
    assert_eq!(target.seeking_age_from, input.seeking_age_from);
    assert_eq!(target.seeking_age_to, input.seeking_age_to);
}
