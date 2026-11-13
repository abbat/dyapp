//! Editable-field consent cannot restore a device removed from verified ordinary authority.
use super::*;
use dyapp_client::{publish, store::Store, Node};
use std::path::Path;

fn owner(peer: &NativePeer) -> Identity {
    let secret = TestStore
        .unwrap(peer.key.wrap(Arc::new(TestStore)).unwrap())
        .unwrap();
    Identity::from_secret(secret.as_slice().try_into().unwrap())
}

fn node() -> (tokio::runtime::Runtime, Multiaddr) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let address = runtime.block_on(start_config(|config| {
        config.roles = vec![Role::Store, Role::Media];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    }));
    (runtime, address)
}

fn hidden_draft(store: &mut Store, identity: &Identity, name: &str) -> Profile {
    for _ in 0..2 {
        let draft = store.profile_draft().unwrap().unwrap();
        publish::hide(store, identity, &draft).unwrap();
    }
    store
        .update_profile_draft(|draft| {
            draft.name = name.into();
            Ok(())
        })
        .unwrap();
    store.profile_draft().unwrap().unwrap()
}

#[test]
fn ordinary_native_profile_save_cannot_restore_a_revoked_device_below_its_hidden_floor() {
    let (runtime, address) = node();
    let peer = retained_person(&address);
    let identity = owner(&peer);
    let mut store =
        Store::open_existing(&Path::new(&peer.dir).join("client.db"), &identity).unwrap();
    let mut remote = runtime
        .block_on(Node::connect(
            address,
            Arc::new(dyapp_p2p_net::Protocols::public()),
        ))
        .unwrap();
    let listed = runtime
        .block_on(remote.get_profile(&identity.peer_id()))
        .unwrap()
        .profile;
    let factual = store.device_listed().unwrap();
    assert_eq!(factual, listed.version);
    let local = hidden_draft(&mut store, &identity, "explicit editable choice");
    let mut revoked = Profile {
        version: listed.version.checked_add(1).unwrap(),
        devices: Vec::new(),
        device_owners: Vec::new(),
        ..listed
    };
    assert!(
        revoked.version < local.version,
        "ordinary removal is below the hidden draft floor"
    );
    runtime
        .block_on(publish_profile_change(&mut remote, &identity, &mut revoked))
        .unwrap();
    let result = peer.open().publish_profile(true);
    let held = runtime
        .block_on(remote.get_profile(&identity.peer_id()))
        .unwrap()
        .profile;
    assert_eq!(
        held.version, revoked.version,
        "a field save must not overwrite device revocation"
    );
    assert_eq!(held.devices.len(), 0);
    assert_eq!(held.device_owners.len(), 0);
    assert!(result.is_err(), "revoked self must be refused");
    assert_eq!(store.device_listed().unwrap(), factual);
    assert_eq!(store.profile_draft().unwrap(), Some(local));
}

#[test]
fn ordinary_native_fields_publish_without_restoring_another_devices_certificate_or_binding() {
    let (runtime, address) = node();
    let peer = retained_person(&address);
    let identity = owner(&peer);
    let mut store =
        Store::open_existing(&Path::new(&peer.dir).join("client.db"), &identity).unwrap();
    let mut remote = runtime
        .block_on(Node::connect(
            address,
            Arc::new(dyapp_p2p_net::Protocols::public()),
        ))
        .unwrap();
    let mut joined = runtime
        .block_on(remote.get_profile(&identity.peer_id()))
        .unwrap()
        .profile;
    let other_key = Identity::generate();
    let other = dyapp_messaging::mls::Device::new(&identity, other_key.public_key(), 2).unwrap();
    publish::list_device(
        &mut joined,
        other.certificate().clone(),
        dyapp_client::owner_binding(&other_key, &identity.public_key()),
    );
    joined.version = joined.version.checked_add(1).unwrap();
    runtime
        .block_on(publish_profile_change(&mut remote, &identity, &mut joined))
        .unwrap();
    store.save_profile_draft(&joined).unwrap();
    let local = hidden_draft(&mut store, &identity, "approved fields");
    let mut revoked = joined;
    publish::unlist_device(&mut revoked, &other_key.public_key()).unwrap();
    revoked.version = revoked.version.checked_add(1).unwrap();
    assert!(revoked.version < local.version);
    runtime
        .block_on(publish_profile_change(&mut remote, &identity, &mut revoked))
        .unwrap();
    let accepted = peer.open().publish_profile(true).unwrap();
    let held = runtime
        .block_on(remote.get_profile(&identity.peer_id()))
        .unwrap()
        .profile;
    assert_eq!(held.name, "approved fields");
    assert_eq!(held.version, accepted.version);
    assert_eq!(held.devices.len(), revoked.devices.len());
    assert!(!held.devices.contains(other.certificate()));
    assert!(held
        .device_owners
        .iter()
        .all(|binding| binding.public_key != other_key.public_key()));
    assert_eq!(store.device_listed().unwrap(), held.version);
}

#[test]
fn refused_native_connection_cannot_record_a_hidden_draft_as_device_listing() {
    let (runtime, address) = node();
    let peer = retained_person(&address);
    let identity = owner(&peer);
    let mut store =
        Store::open_existing(&Path::new(&peer.dir).join("client.db"), &identity).unwrap();
    let factual = store.device_listed().unwrap();
    let before = hidden_draft(&mut store, &identity, "unpublished choice");
    assert!(before.version > factual);
    super::offline::set_seed(&peer, super::offline::refusing_seed(&runtime));
    assert!(peer.open().publish_profile(true).is_err());
    assert_eq!(store.device_listed().unwrap(), factual);
    assert_eq!(store.profile_draft().unwrap(), Some(before));
}
