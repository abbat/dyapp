use super::*;
use crate::ProfileCondition;
use dyapp_identity::Identity;

#[test]
fn search_purpose_survives_resigning_and_cannot_authorize_ordinary_storage() {
    let owner = Identity::from_secret(&[7; 32]);
    let record = Profile::search_tombstone(2, 0, 0).sign(&owner);
    assert!(record.verify(Domain::SearchWithdrawal).is_ok());
    assert!(record.verify(Domain::Profile).is_err());
    assert!(record.verify(Domain::ProfileConditional).is_err());
    assert!(matches!(verify(&record), Err(Error::Signature(_))));
    let verified = verify_search(&record).unwrap();
    assert_eq!(verified.profile.sign(&owner), record);
    assert!(verified.check_storage_authorization().is_err());
    let mut stripped = verified.profile;
    stripped.search_only = false;
    let detached = SignedRecord {
        payload: stripped.encode_to_vec(),
        ..record
    };
    assert!(verify(&detached).is_err());
    assert!(verify_search(&detached).is_err());
}

#[test]
fn legacy_tombstones_are_readable_but_only_conditional_deletion_is_admissible() {
    let owner = Identity::from_secret(&[7; 32]);
    let legacy = Profile::tombstone(2, 0, 0).sign(&owner);
    assert!(matches!(verify(&legacy), Err(Error::Invalid(_))));
    assert!(verify_historical(&legacy).is_ok());
    assert!(verify_search(&legacy).is_ok());
    let source = Profile {
        version: 1,
        ..Default::default()
    }
    .sign(&owner);
    let deletion = Profile {
        previous: Some(ProfileCondition::observed(Some(&source)).unwrap()),
        ..Profile::tombstone(2, 0, 0)
    }
    .sign(&owner);
    assert!(verify(&deletion)
        .unwrap()
        .check_storage_authorization()
        .is_ok());
    assert!(verify_search(&deletion).is_ok());
}

#[test]
fn search_purpose_cannot_carry_a_live_profile_or_an_ordinary_fence() {
    let owner = Identity::from_secret(&[7; 32]);
    let live = Profile {
        version: 1,
        search_only: true,
        ..Default::default()
    };
    assert!(verify_search(&live.sign(&owner)).is_err());
    let fenced = Profile {
        previous: Some(ProfileCondition::default()),
        ..Profile::search_tombstone(1, 0, 0)
    };
    assert!(verify_search(&fenced.sign(&owner)).is_err());
    let wrong_domain = owner.sign(
        Domain::Profile,
        Profile::search_tombstone(1, 0, 0).encode_to_vec(),
    );
    assert!(verify_search(&wrong_domain).is_err());
}
