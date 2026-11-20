//! Mailbox authority comes from the live common context, never an editable local draft.
use super::favorites::Fixture;
use super::*;
use dyapp_profile::Profile;

fn unpublished_owner(f: &Fixture) -> dyapp_profile::VerifiedProfile {
    let session = f.chats.session.lock().unwrap();
    let mut draft = Profile {
        version: 99,
        name: "Unpublished local draft".into(),
        devices: vec![session.certificate().unwrap()],
        ..Default::default()
    };
    session.store().save_profile_draft(&draft).unwrap();
    // The legacy outbound helper signs this counterexample; mailbox authority must not use it.
    let signed_draft = own_profile(session.store(), &f.key.identity)
        .unwrap()
        .unwrap();
    assert_eq!(signed_draft.profile.version, 99);
    draft.version = 3;
    draft.name = "Confirmed remote owner".into();
    dyapp_profile::verify(&draft.sign(&f.key.identity)).unwrap()
}

#[test]
fn native_mailbox_authority_never_promotes_the_editable_owner_draft() {
    let f = Fixture::new();
    let chats = &f.chats;
    let generation = chats.contact_context.lock().unwrap().generation();
    let remote = unpublished_owner(&f);
    assert!(chats
        .receive_profiles(&generation)
        .unwrap()
        .all()
        .is_empty());
    {
        let session = chats.session.lock().unwrap();
        let mut context = chats.contact_context.lock().unwrap();
        context
            .accept_own(&generation, &session, remote.clone(), 10)
            .unwrap();
    }
    let held = chats.receive_profiles(&generation).unwrap();
    assert_eq!(held.get(&f.key.peer_id()), Some(&remote));
    {
        let session = chats.session.lock().unwrap();
        let draft = session.store().profile_draft().unwrap().unwrap();
        assert_eq!(draft.version, 99);
    }
    let current = {
        let mut context = chats.contact_context.lock().unwrap();
        context.clear();
        context.generation()
    };
    let failure = chats.receive_profiles(&generation).unwrap_err();
    assert_eq!(failure.to_string(), "notConnected");
    assert!(chats.receive_profiles(&current).unwrap().all().is_empty());
}

#[test]
fn native_mailbox_snapshot_uses_newly_admitted_contact_authority() {
    let f = Fixture::new();
    let peer_key = Identity::try_generate().unwrap();
    let peer = peer_key.peer_id();
    let old = dyapp_profile::verify(
        &Profile {
            version: 1,
            ..Default::default()
        }
        .sign(&peer_key),
    )
    .unwrap();
    let current = dyapp_profile::verify(
        &Profile {
            version: 2,
            ..Default::default()
        }
        .sign(&peer_key),
    )
    .unwrap();
    let generation = {
        let mut context = f.chats.contact_context.lock().unwrap();
        context.accept(old.clone()).unwrap();
        context.generation()
    };
    let before = f.chats.receive_profiles(&generation).unwrap();
    {
        let session = f.chats.session.lock().unwrap();
        assert!(f
            .chats
            .contact_context
            .lock()
            .unwrap()
            .accept_contact_if_current(&generation, session.store(), &peer, current.clone())
            .unwrap());
    }
    assert_eq!(before.get(&peer), Some(&old));
    assert_eq!(
        f.chats.receive_profiles(&generation).unwrap().get(&peer),
        Some(&current)
    );
}
