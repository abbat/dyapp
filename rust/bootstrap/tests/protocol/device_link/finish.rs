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
