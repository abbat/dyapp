mod behavior;
mod server;

use super::*;
use crate::slot_registry_tests::{root, Credentials, Wrapping};
use crate::{MobileSlotLayout, MobileSlotSelection};
use dyapp_profile::Profile;
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    registry: Arc<MobileIdentityRegistry>,
    slot: MobileSlotSelection,
    worker: Arc<MobileLinkRecovery>,
    server: server::Server,
    directory: PathBuf,
}
impl Fixture {
    fn new(layout: MobileSlotLayout) -> Self {
        let root = root();
        let registry = MobileIdentityRegistry::new(
            root.to_str().unwrap().into(),
            layout,
            Arc::new(Credentials::default()),
        );
        let slot = registry.create().unwrap();
        drop(
            registry
                .open_chats(slot.slot.id.clone(), slot.key.peer_id(), Arc::new(Wrapping))
                .unwrap(),
        );
        let worker = MobileLinkRecovery::new(
            registry.clone(),
            slot.slot.id.clone(),
            slot.key.peer_id(),
            Arc::new(Wrapping),
        );
        let admitted = worker.owner.open().unwrap();
        let record = prepare_review_state(&admitted);
        let directory = admitted.operation.slot().dir.clone();
        drop(admitted);
        let server = server::Server::new(record);
        dyapp_identity::network::save(
            &directory,
            dyapp_identity::network::Settings::Active,
            &server.network,
        )
        .unwrap();
        Self {
            root,
            registry,
            slot,
            worker,
            server,
            directory,
        }
    }

    fn store(&self) -> Store {
        Store::open_existing(&self.directory.join("client.db"), &self.slot.key.identity).unwrap()
    }

    fn choices() -> Vec<LinkReviewChoice> {
        vec![LinkReviewChoice {
            field: LinkReviewField::Name,
            source: LinkReviewSource::Local,
        }]
    }
}

fn prepare_review_state(admitted: &Admitted) -> SignedRecord {
    let operation = &admitted.operation;
    let store = admitted.session.store();
    let remote = Profile {
        version: 3,
        name: "remote".into(),
        devices: vec![admitted.session.certificate().unwrap()],
        device_owners: vec![dyapp_client::owner_binding(
            operation.device(),
            &operation.identity().public_key(),
        )],
        ..Default::default()
    };
    let local = Profile {
        version: 4,
        name: "local".into(),
        ..remote.clone()
    };
    store.save_profile_draft(&local).unwrap();
    store.set_profile_hidden(true).unwrap();
    store.set_link_pending(b"original link").unwrap();
    store.note_device_listed(remote.version).unwrap();
    store
        .hold_search_profile(&Profile::tombstone(local.version, 0, 0).sign(operation.identity()))
        .unwrap();
    remote.sign(operation.identity())
}

fn advanced_profile() -> Profile {
    Profile {
        version: 7,
        name: "exact".into(),
        income_from: Some(1_u64 << 40),
        income_to: Some(1_u64 << 41),
        has_kids: Some(false),
        wants_more_kids: None,
        interests: vec!["one".into(), "two".into()],
        education_level: 999,
        profession: "profession".into(),
        company: "company".into(),
        industry: "industry".into(),
        career_stage: "stage".into(),
        work_life_balance: "balance".into(),
        photos: vec![dyapp_profile::Photo {
            thumbnail: vec![1; 32],
            full: vec![vec![2; 32], vec![3; 32]],
        }],
        ..Default::default()
    }
}

#[test]
fn projection_preserves_advanced_values_and_all_original_photo_chunks() {
    let view = LinkReviewProfile::from(&advanced_profile());
    assert_eq!(view.version, 7);
    assert_eq!(view.fields.name, "exact");
    assert_eq!(
        (view.income_from, view.income_to),
        (Some(1_u64 << 40), Some(1_u64 << 41))
    );
    assert_eq!((view.has_kids, view.wants_more_kids), (Some(false), None));
    assert_eq!(view.interests, ["one", "two"]);
    assert_eq!(view.education_level, 999);
    assert_eq!(
        (
            view.profession,
            view.company,
            view.industry,
            view.career_stage,
            view.work_life_balance
        ),
        (
            "profession".into(),
            "company".into(),
            "industry".into(),
            "stage".into(),
            "balance".into()
        )
    );
    assert_eq!(
        view.photos,
        [LinkReviewPhoto {
            thumbnail: vec![1; 32],
            full: vec![vec![2; 32], vec![3; 32]]
        }]
    );
}
