use super::*;
use crate::IdentityKey;
use dyapp_identity::{
    network::{self, Settings},
    Identity,
};
use dyapp_messaging::mls::Device;
use dyapp_profile::Profile;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

struct Fixture {
    chats: Arc<ChatSession>,
    dir: PathBuf,
}

impl Fixture {
    fn new(secret: u8) -> Self {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/native-verification")
            .join(Identity::try_generate().unwrap().peer_id());
        std::fs::create_dir_all(&dir).unwrap();
        let key = |secret| {
            Arc::new(IdentityKey {
                identity: Identity::from_secret(&[secret; 32]),
            })
        };
        let chats = ChatSession::from_keys(
            dir.clone(),
            key(secret),
            key(9),
            Zeroizing::new([8; 32]),
            None,
        )
        .unwrap();
        network::save(
            &dir,
            Settings::Active,
            &Network {
                name: "A".into(),
                seeds: vec![],
            },
        )
        .unwrap();
        Self { chats, dir }
    }

    fn confirm(&self, profile: VerifiedProfile) -> Result<FoundProfile, CoreError> {
        self.chats
            .confirm_contact_with(&profile.peer_id.clone(), |_, _| Ok(profile))
    }
}

fn profile(version: u64, device: bool) -> VerifiedProfile {
    let owner = Identity::from_secret(&[2; 32]);
    let devices = if device {
        vec![
            Device::new(&owner, Identity::from_secret(&[3; 32]).public_key(), 1)
                .unwrap()
                .certificate()
                .clone(),
        ]
    } else {
        vec![]
    };
    dyapp_profile::verify(
        &Profile {
            version,
            devices,
            ..Default::default()
        }
        .sign(&owner),
    )
    .unwrap()
}

#[test]
fn native_state_notice_acknowledgement_and_unverify_share_core_authority() {
    let f = Fixture::new(1);
    let current = profile(1, false);
    let peer = current.peer_id.clone();
    assert_eq!(f.confirm(current).unwrap().version, 1);
    assert!(f.chats.contact_verification(peer.clone()).unwrap().verified);
    f.chats
        .contact_context
        .lock()
        .unwrap()
        .accept(profile(2, true))
        .unwrap();
    assert_eq!(
        f.chats.contact_verification(peer.clone()).unwrap(),
        ContactVerificationState {
            verified: false,
            new_device: true
        }
    );
    f.chats.acknowledge_contact_devices(peer.clone()).unwrap();
    assert_eq!(
        f.chats.contact_verification(peer.clone()).unwrap(),
        ContactVerificationState {
            verified: false,
            new_device: false
        }
    );
    f.confirm(profile(2, true)).unwrap();
    f.chats.unverify_contact(peer.clone()).unwrap();
    assert!(!f.chats.contact_verification(peer.clone()).unwrap().verified);
    assert!(
        !Fixture::new(4)
            .chats
            .contact_verification(peer)
            .unwrap()
            .verified
    );
}

#[test]
fn activation_during_fetch_rejects_a_switch_back_without_deadlock_or_a_mark() {
    let f = Fixture::new(1);
    let current = profile(1, false);
    let peer = current.peer_id.clone();
    let result = f.chats.confirm_contact_with(&peer, |network, requested| {
        assert_eq!(network.name, "A");
        assert_eq!(requested, peer);
        let shared = contact_verification::shared(&f.dir).unwrap();
        for name in ["B", "A"] {
            let mut context = shared.try_lock().unwrap();
            context.clear();
            network::save(
                &f.dir,
                Settings::Active,
                &Network {
                    name: name.into(),
                    seeds: vec![],
                },
            )
            .unwrap();
        }
        Ok(current)
    });
    assert_eq!(result.unwrap_err().to_string(), "notConnected");
    assert!(!f.chats.contact_verification(peer).unwrap().verified);
}

#[test]
fn invalid_own_and_uppercase_targets_fail_before_fetch_or_mutation() {
    let f = Fixture::new(1);
    let own = f.chats.key.peer_id();
    let peer = profile(1, false).peer_id;
    for target in ["invalid".into(), own, peer.to_ascii_uppercase()] {
        assert!(f
            .chats
            .confirm_contact_with(&target, |_, _| panic!("invalid target fetched"))
            .is_err());
        assert!(f.chats.contact_verification(target.clone()).is_err());
        assert!(f.chats.unverify_contact(target.clone()).is_err());
        assert!(f.chats.acknowledge_contact_devices(target).is_err());
    }
    assert!(!f.chats.contact_verification(peer).unwrap().verified);
}

#[test]
fn wrong_profiles_and_failed_fetches_preserve_the_existing_mark() {
    let f = Fixture::new(1);
    let current = profile(1, false);
    let peer = current.peer_id.clone();
    f.confirm(current).unwrap();
    assert!(f
        .chats
        .confirm_contact_with(&peer, |_, _| Err(CoreError::Failed(
            "controlled fetch failure".into()
        )))
        .is_err());
    let foreign = Identity::from_secret(&[4; 32]);
    let wrong = dyapp_profile::verify(
        &Profile {
            version: 2,
            ..Default::default()
        }
        .sign(&foreign),
    )
    .unwrap();
    assert_eq!(
        f.chats
            .confirm_contact_with(&peer, |_, _| Ok(wrong))
            .unwrap_err()
            .to_string(),
        "unknownPeer"
    );
    assert!(f.chats.contact_verification(peer).unwrap().verified);
}

#[test]
fn native_session_failures_are_errors_and_cannot_remove_a_committed_mark() {
    let f = Fixture::new(1);
    let current = profile(1, false);
    let peer = current.peer_id.clone();
    f.confirm(current).unwrap();
    let chats = Arc::clone(&f.chats);
    assert!(std::thread::spawn(move || {
        let _guard = chats.session.lock().unwrap();
        panic!("poison isolated verification session");
    })
    .join()
    .is_err());
    assert!(f.chats.contact_verification(peer.clone()).is_err());
    assert!(f.chats.unverify_contact(peer.clone()).is_err());
    assert!(f.chats.acknowledge_contact_devices(peer.clone()).is_err());
    let reopened = ChatSession::from_keys(
        f.dir,
        Arc::clone(&f.chats.key),
        Arc::clone(&f.chats.device_key),
        Zeroizing::new([8; 32]),
        None,
    )
    .unwrap();
    assert!(reopened.contact_verification(peer).unwrap().verified);
}

#[test]
fn the_public_confirmation_export_requires_a_reachable_node() {
    let f = Fixture::new(1);
    let peer = profile(1, false).peer_id;
    assert_eq!(
        f.chats
            .confirm_contact(peer.clone())
            .unwrap_err()
            .to_string(),
        "notConnected"
    );
    assert!(!f.chats.contact_verification(peer).unwrap().verified);
}
