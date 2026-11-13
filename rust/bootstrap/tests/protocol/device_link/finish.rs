//! Hidden link completion must remember ordinary readback, never its search tombstone.
use super::*;

struct HiddenLink {
    owner: Person,
    laptop: Person,
    path: std::path::PathBuf,
    state: Vec<u8>,
}
impl HiddenLink {
    async fn new() -> Self {
        let address = start_config(|config| {
            config.roles = vec![Role::Store];
            config.limits.requests_per_second = 1000;
            config.limits.min_free_mb = 0;
        })
        .await;
        let owner = Person::new(&address).await;
        let phone = Store::open(&store_path("hidden-finish-phone"), &owner.identity).unwrap();
        phone.set_profile_hidden(true).unwrap();
        let laptop =
            Person::device(&address, Identity::from_secret(&owner.identity.secret()), 2).await;
        Self {
            owner,
            laptop,
            path: store_path("hidden-finish-laptop"),
            state: phone.link_state().unwrap(),
        }
    }

    async fn prepare_legacy_partial_link(&mut self) -> Profile {
        let mut remote = self
            .owner
            .node
            .get_profile(&self.owner.identity.peer_id())
            .await
            .unwrap()
            .profile;
        let session = Session::open_or_create(
            self.store(),
            &self.laptop.identity,
            &self.laptop.device_key,
            &SEAL,
        )
        .unwrap();
        let binding = dyapp_client::owner_binding(
            &self.laptop.device_key,
            &self.laptop.identity.public_key(),
        );
        session.list_self(&mut remote, binding).unwrap();
        remote.version = remote.version.checked_add(1).unwrap();
        self.owner
            .node
            .publish_profile(remote.sign(&self.owner.identity))
            .await
            .unwrap();
        drop(session);
        // Write the pre-receipt completion layout through the old storage APIs, never erase a receipt.
        let store = self.store();
        store.set_profile_hidden(true).unwrap();
        let current = Profile {
            version: remote.version.checked_add(1).unwrap(),
            ..remote.clone()
        };
        store.save_profile_draft(&current).unwrap();
        store
            .hold_search_profile(
                &Profile::tombstone(current.version, current.pow_nonce, current.pow_bits)
                    .sign(&self.laptop.identity),
            )
            .unwrap();
        store.note_device_listed(remote.version).unwrap();
        store.set_link_pending(&self.state).unwrap();
        remote
    }

    async fn finish(&mut self) -> Profile {
        let store = self.store();
        store.set_link_pending(&self.state).unwrap();
        assert!(Session::finish_link(
            store,
            &mut self.laptop.node,
            &self.laptop.identity,
            &self.laptop.device_key,
            2,
            &SEAL,
            1_000_000,
        )
        .await
        .unwrap());
        self.owner
            .node
            .get_profile(&self.owner.identity.peer_id())
            .await
            .unwrap()
            .profile
    }

    fn store(&self) -> Store {
        Store::open(&self.path, &self.laptop.identity).unwrap()
    }

    fn check_listed(&self, published: &Profile) {
        let store = self.store();
        assert!(store.profile_hidden().unwrap());
        assert!(store.link_pending().unwrap().is_none());
        assert_eq!(store.device_listed().unwrap(), published.version);
        assert_eq!(
            store.profile_draft().unwrap().unwrap().version,
            published.version.checked_add(1).unwrap()
        );
    }
}

#[tokio::test]
async fn reviewed_pending_fields_stay_private_while_the_original_link_finishes() {
    let mut f = Box::pin(HiddenLink::new()).await;
    let remote = f.prepare_legacy_partial_link().await;
    let mut store = f.store();
    store
        .update_profile_draft(|draft| {
            draft.name = "chosen private draft".into();
            Ok(())
        })
        .unwrap();
    let session = Session::open(store, &SEAL).unwrap();
    let review = publish::review_adoption(session.store(), &mut f.laptop.node, &f.laptop.identity)
        .await
        .unwrap();
    let saved = publish::confirm_adoption_review(
        &session,
        &mut f.laptop.node,
        &f.laptop.identity,
        &review,
        &[(publish::AdoptionField::Name, publish::AdoptionSource::Local)],
    )
    .await
    .unwrap();
    assert_eq!(saved.published, remote);
    assert_eq!(saved.draft.name, "chosen private draft");
    assert!(saved.pending && saved.hidden);
    assert!(session.store().link_pending().unwrap().is_some());
    drop(session);
    assert_eq!(f.finish().await, remote);
    let completed = f.store();
    assert!(completed.link_pending().unwrap().is_none());
    assert_eq!(
        completed.profile_draft().unwrap(),
        Some(saved.draft.clone())
    );
    assert_eq!(completed.device_listed().unwrap(), remote.version);
    drop(completed);
    assert_eq!(f.finish().await, remote);
    assert_eq!(f.store().profile_draft().unwrap(), Some(saved.draft));
}

#[tokio::test]
async fn ambiguous_legacy_link_reports_review_without_replacing_pending_edits() {
    let mut f = Box::pin(HiddenLink::new()).await;
    let remote = f.prepare_legacy_partial_link().await;
    let mut store = f.store();
    store
        .update_profile_draft(|draft| {
            draft.name = "pending before upgrade".into();
            Ok(())
        })
        .unwrap();
    let before = store.profile_draft().unwrap();
    let result = Session::finish_link(
        store,
        &mut f.laptop.node,
        &f.laptop.identity,
        &f.laptop.device_key,
        2,
        &SEAL,
        1_000_000,
    )
    .await;
    assert!(matches!(
        result,
        Err(dyapp_client::session::Error::Publish(
            Failure::AdoptionReviewRequired
        ))
    ));
    let restored = f.store();
    assert_eq!(restored.profile_draft().unwrap(), before);
    assert_eq!(restored.device_listed().unwrap(), remote.version);
    assert!(restored.profile_hidden().unwrap());
    assert!(restored.link_pending().unwrap().is_some());
    let session = Session::open(restored, &SEAL).unwrap();
    assert!(remote.devices.contains(&session.certificate().unwrap()));
    assert_eq!(
        f.owner
            .node
            .get_profile(&f.owner.identity.peer_id())
            .await
            .unwrap()
            .profile,
        remote
    );
}

#[tokio::test]
async fn a_pre_receipt_hidden_partial_link_finishes_after_store_reopen() {
    let mut f = Box::pin(HiddenLink::new()).await;
    let remote = f.prepare_legacy_partial_link().await;
    assert!(f.store().link_pending().unwrap().is_some());
    let completed = f.finish().await;
    assert_eq!(
        completed, remote,
        "legacy recovery must not spend a profile version"
    );
    f.check_listed(&completed);
    let retried = f.finish().await;
    assert_eq!(retried, remote);
    f.check_listed(&retried);
}

#[tokio::test]
async fn hidden_link_completion_and_retry_record_exact_readback_and_detect_next_revocation() {
    let mut f = Box::pin(HiddenLink::new()).await;
    let published = f.finish().await;
    f.check_listed(&published);
    let retried = f.finish().await;
    assert_eq!(retried, published, "retry must not spend a profile version");
    f.check_listed(&retried);
    let revoked = Profile {
        version: published.version.checked_add(1).unwrap(),
        devices: vec![f.owner.device.certificate().clone()],
        device_owners: Vec::new(),
        ..published
    };
    f.owner
        .node
        .publish_profile(revoked.sign(&f.owner.identity))
        .await
        .unwrap();
    let remote = f
        .owner
        .node
        .get_profile(&f.owner.identity.peer_id())
        .await
        .unwrap();
    let session = Session::open(f.store(), &SEAL).unwrap();
    assert!(session.own_device_revoked(&remote.profile).unwrap());
}
