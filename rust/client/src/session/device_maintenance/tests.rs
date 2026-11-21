use super::*;
use crate::contact_verification::Context;
use crate::store::{tests::temp_db_path, Store};
use dyapp_identity::{Identity, SignedRecord};
use dyapp_messaging::mls::Device;
use dyapp_profile::Profile;
use std::path::PathBuf;

const SEAL: [u8; 32] = [7; 32];

struct Fixture {
    identity: Identity,
    certificate: SignedRecord,
    path: PathBuf,
    session: Session,
    known: Profiles,
}

impl Fixture {
    fn new() -> Self {
        let identity = Identity::try_generate().unwrap();
        let device =
            Device::new(&identity, Identity::try_generate().unwrap().public_key(), 1).unwrap();
        let path = temp_db_path();
        let mut store = Store::open(&path, &identity).unwrap();
        store
            .stage_outbound(&device.seal(&SEAL).unwrap(), &[])
            .unwrap();
        store
            .save_profile_draft(&Profile {
                version: 99,
                name: "Unpublished local edit".into(),
                ..Default::default()
            })
            .unwrap();
        Self {
            identity,
            certificate: device.certificate().clone(),
            path,
            session: Session::open(store, &SEAL).unwrap(),
            known: Profiles::default(),
        }
    }

    fn profile(&self, version: u64, listed: bool) -> VerifiedProfile {
        let profile = Profile {
            version,
            name: format!("Remote {version}"),
            devices: if listed {
                vec![self.certificate.clone()]
            } else {
                vec![]
            },
            ..Default::default()
        };
        dyapp_profile::verify(&profile.sign(&self.identity)).unwrap()
    }

    fn admit(&mut self, profile: VerifiedProfile, now: u64) -> Result<VerifiedProfile, Error> {
        self.session
            .admit_own_profile(&mut self.known, profile, now)
    }
}

#[tokio::test]
async fn maintenance_reports_invalidated_state_and_recovers_from_the_committed_snapshot() {
    let mut fixture = Fixture::new();
    let own = fixture.profile(3, true);
    let draft = fixture.session.store().profile_draft().unwrap();
    let sealed = fixture.session.store().sealed_device().unwrap();
    // Staging failures discard the in-memory device; maintenance must require a reopen too.
    fixture.session.device = None;
    let error = fixture
        .session
        .maintain_own_devices(&mut [], &own, std::slice::from_ref(&own), 10, &SEAL)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Invalidated), "{error}");
    assert!(fixture.session.own_group().is_none());
    assert_eq!(fixture.session.store().profile_draft().unwrap(), draft);
    assert_eq!(fixture.session.store().sealed_device().unwrap(), sealed);
    drop(fixture.session);
    let store = Store::open(&fixture.path, &fixture.identity).unwrap();
    let mut reopened = Session::open(store, &SEAL).unwrap();
    // A valid singleton needs no network or own group; reopening restores that no-op success.
    reopened
        .maintain_own_devices(&mut [], &own, std::slice::from_ref(&own), 11, &SEAL)
        .await
        .unwrap();
    assert!(reopened.own_group().is_none());
    assert_eq!(reopened.store().profile_draft().unwrap(), draft);
    assert_eq!(reopened.store().sealed_device().unwrap(), sealed);
}

#[test]
fn owner_readback_keeps_ordinary_floor_separate_from_unpublished_draft() {
    let mut fixture = Fixture::new();
    let draft = fixture.session.store().profile_draft().unwrap();
    let remote = fixture.profile(3, true);
    assert_eq!(fixture.admit(remote.clone(), 10).unwrap(), remote);
    assert_eq!(fixture.session.store().own_profile_seen().unwrap(), 3);
    assert_eq!(fixture.session.store().device_listed().unwrap(), 3);
    assert_eq!(fixture.session.store().profile_draft().unwrap(), draft);
    assert_eq!(fixture.admit(fixture.profile(2, true), 11).unwrap(), remote);
    let mut fork = remote.profile.clone();
    fork.name = "Same-version fork".into();
    let fork = dyapp_profile::verify(&fork.sign(&fixture.identity)).unwrap();
    assert_eq!(fixture.admit(fork, 12).unwrap(), remote);
    assert_eq!(fixture.session.store().profile_draft().unwrap(), draft);
}

#[test]
fn known_revocation_remains_refused_and_older_authority_fails_after_reopen() {
    let mut fixture = Fixture::new();
    let listed = fixture.profile(3, true);
    fixture.admit(listed.clone(), 10).unwrap();
    assert!(matches!(
        fixture.admit(fixture.profile(4, false), 11),
        Err(Error::Revoked)
    ));
    assert_eq!(
        fixture
            .known
            .get(&fixture.identity.peer_id())
            .unwrap()
            .profile
            .version,
        4
    );
    assert!(matches!(
        fixture.admit(listed.clone(), 12),
        Err(Error::Revoked)
    ));
    let Fixture {
        identity,
        path,
        session,
        ..
    } = fixture;
    drop(session);
    let reopened = Session::open(Store::open(&path, &identity).unwrap(), &SEAL).unwrap();
    let mut empty = Profiles::default();
    assert!(matches!(
        reopened.admit_own_profile(&mut empty, listed, 13),
        Err(Error::Client(crate::Error::Rollback))
    ));
    assert_eq!(empty.all().len(), 0);
    assert_eq!(reopened.store().own_profile_seen().unwrap(), 4);
    assert_eq!(reopened.store().device_listed().unwrap(), 3);
}

#[test]
fn foreign_owner_cannot_enter_cache_or_advance_listing_clocks() {
    let mut fixture = Fixture::new();
    let foreign = Identity::try_generate().unwrap();
    let profile = dyapp_profile::verify(
        &Profile {
            version: 8,
            ..Default::default()
        }
        .sign(&foreign),
    )
    .unwrap();
    assert!(matches!(
        fixture.admit(profile, 10),
        Err(Error::Store(crate::store::Error::WrongIdentity))
    ));
    assert_eq!(fixture.known.all().len(), 0);
    assert_eq!(fixture.session.store().own_profile_seen().unwrap(), 0);
    assert_eq!(fixture.session.store().device_listed().unwrap(), 0);
}

#[test]
fn obsolete_network_generation_cannot_admit_even_a_newer_owner_profile() {
    let fixture = Fixture::new();
    let mut context = Context::default();
    let previous = context.generation();
    context.clear();
    assert!(context
        .accept_own(&previous, &fixture.session, fixture.profile(3, true), 10)
        .is_err());
    assert_eq!(context.profiles().all().len(), 0);
    assert_eq!(fixture.session.store().own_profile_seen().unwrap(), 0);
    let current = context.generation();
    assert_eq!(
        context
            .accept_own(&current, &fixture.session, fixture.profile(3, true), 11)
            .unwrap()
            .profile
            .version,
        3
    );
}

#[test]
fn account_tombstone_retains_common_terminal_precedence_over_live_forks() {
    let mut fixture = Fixture::new();
    fixture.admit(fixture.profile(5, true), 10).unwrap();
    let source = Profile {
        version: 1,
        ..Default::default()
    }
    .sign(&fixture.identity);
    let deleted = Profile {
        previous: Some(dyapp_profile::ProfileCondition::observed(Some(&source)).unwrap()),
        ..Profile::tombstone(2, 0, 0)
    };
    let terminal = dyapp_profile::verify(&deleted.sign(&fixture.identity)).unwrap();
    assert!(fixture.admit(terminal, 11).unwrap().profile.deleted);
    assert!(
        fixture
            .admit(fixture.profile(6, true), 12)
            .unwrap()
            .profile
            .deleted
    );
    assert_eq!(fixture.session.store().own_profile_seen().unwrap(), 5);
}

#[test]
fn search_withdrawal_never_becomes_owner_device_authority() {
    let mut fixture = Fixture::new();
    let ordinary = fixture.profile(3, true);
    fixture.admit(ordinary.clone(), 10).unwrap();
    let withdrawal =
        dyapp_profile::verify_search(&Profile::search_tombstone(8, 0, 0).sign(&fixture.identity))
            .unwrap();
    assert!(matches!(
        fixture.admit(withdrawal, 11),
        Err(Error::Client(crate::Error::Input(_)))
    ));
    assert_eq!(
        fixture.known.get(&fixture.identity.peer_id()),
        Some(&ordinary)
    );
    assert_eq!(fixture.session.store().own_profile_seen().unwrap(), 3);
}
