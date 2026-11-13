use super::*;
use dyapp_identity::Identity;

fn source(owner: &Identity, version: u64, name: &str) -> SignedRecord {
    Profile {
        version,
        name: name.into(),
        ..Default::default()
    }
    .sign(owner)
}

fn target(owner: &Identity, source: Option<&SignedRecord>) -> SignedRecord {
    Profile {
        version: 10,
        name: "approved".into(),
        previous: Some(ProfileCondition::observed(source).unwrap()),
        ..Default::default()
    }
    .sign(owner)
}

#[test]
fn signature_cannot_be_downgraded_or_detached_and_resigning_keeps_the_domain() {
    let owner = Identity::from_secret(&[7; 32]);
    let record = target(&owner, None);
    assert!(record.verify(Domain::ProfileConditional).is_ok());
    assert!(record.verify(Domain::Profile).is_err());
    let profile = verify(&record).unwrap().profile;
    assert_eq!(profile.sign(&owner), record);
    let mut stripped = profile;
    stripped.previous = None;
    let stripped = SignedRecord {
        payload: stripped.encode_to_vec(),
        ..record.clone()
    };
    assert!(verify(&stripped).is_err());
    let legacy_domain = owner.sign(Domain::Profile, record.payload.clone());
    assert!(verify(&legacy_domain).is_err());
}

#[test]
fn fence_accepts_observed_missing_and_older_replicas_but_not_newer_or_forked_state() {
    let owner = Identity::from_secret(&[7; 32]);
    let observed = source(&owner, 2, "observed");
    let target = verify(&target(&owner, Some(&observed))).unwrap();
    assert!(target.check_condition(None).is_ok());
    assert!(target
        .check_condition(Some(&source(&owner, 1, "older replica")))
        .is_ok());
    assert!(target.check_condition(Some(&observed)).is_ok());
    assert_eq!(
        target.check_condition(Some(&source(&owner, 2, "fork"))),
        Err(Error::Stale)
    );
    assert_eq!(
        target.check_condition(Some(&source(&owner, 3, "intervening"))),
        Err(Error::Stale)
    );
}

#[test]
fn absent_source_does_not_authorize_replacing_any_existing_profile() {
    let owner = Identity::from_secret(&[7; 32]);
    let target = verify(&target(&owner, None)).unwrap();
    assert!(target.check_condition(None).is_ok());
    assert_eq!(
        target.check_condition(Some(&source(&owner, 1, "appeared"))),
        Err(Error::Stale)
    );
}

#[test]
fn condition_binds_original_unknown_payload_fields() {
    let owner = Identity::from_secret(&[7; 32]);
    let original = source(&owner, 2, "observed");
    let mut payload = original.payload.clone();
    // Unknown field 100, varint 1: valid signed bytes that a decoded/re-signed profile loses.
    payload.extend_from_slice(&[0xa0, 0x06, 0x01]);
    let original = owner.sign(Domain::Profile, payload);
    let target = verify(&target(&owner, Some(&original))).unwrap();
    assert!(target.check_condition(Some(&original)).is_ok());
    let rebuilt = verify(&original).unwrap().profile.sign(&owner);
    assert_eq!(target.check_condition(Some(&rebuilt)), Err(Error::Stale));
}

#[test]
fn unauthenticated_payload_is_rejected_before_decoding_and_domain_must_match_metadata() {
    let owner = Identity::from_secret(&[7; 32]);
    let mut forged = source(&owner, 1, "valid");
    forged.payload = vec![0xff; 4];
    assert!(matches!(verify(&forged), Err(Error::Signature(_))));
    let missing = owner.sign(
        Domain::ProfileConditional,
        Profile {
            version: 1,
            ..Default::default()
        }
        .encode_to_vec(),
    );
    assert!(matches!(verify(&missing), Err(Error::Signature(_))));
}

#[test]
fn invalid_conditions_and_conditional_tombstones_are_checked() {
    let owner = Identity::from_secret(&[7; 32]);
    for previous in [
        ProfileCondition {
            version: 0,
            record_hash: vec![1; 32],
        },
        ProfileCondition {
            version: 1,
            record_hash: Vec::new(),
        },
        ProfileCondition {
            version: 10,
            record_hash: vec![1; 32],
        },
    ] {
        let profile = Profile {
            version: 10,
            previous: Some(previous),
            ..Default::default()
        };
        assert!(verify(&profile.sign(&owner)).is_err());
    }
    let tombstone = Profile {
        previous: Some(ProfileCondition::default()),
        ..Profile::tombstone(1, 0, 0)
    };
    assert!(verify(&tombstone.sign(&owner)).is_ok());
}
