//! The shared receiver hands an actual wire record to the retained registry installer.
use super::*;
use dyapp_client::identity_slots::{
    Credential, KeyReader, KeyRemover, KeyWriter, Layout, Registry, Slot, Storage,
};
use dyapp_identity::network::{self, Network, Settings};
use std::fs;
use std::io::Write;
use std::sync::Arc;
use zeroize::Zeroizing;

#[path = "registry/receiver.rs"]
mod receiver;

struct Files;

impl KeyReader for Files {
    fn contains(&self, _: &Slot) -> Result<bool, String> {
        Ok(false)
    }
    fn open(&self, _: &Slot, _: Option<&[u8]>) -> Result<Option<Identity>, String> {
        Ok(None)
    }
}
impl KeyWriter for Files {
    fn store(&self, _: &Slot, _: &Identity) -> Result<Option<Vec<u8>>, String> {
        unreachable!("file storage writes directly")
    }
}
impl KeyRemover for Files {
    fn credential_present(&self, _: &Slot, _: Credential) -> Result<bool, String> {
        Ok(false)
    }
    fn erase_credential(&self, _: &Slot, _: Credential) -> Result<(), String> {
        Ok(())
    }
}

fn persist_device(slot: &Slot, device: &Identity) -> Result<(), String> {
    let path = slot.dir.join("device.key");
    let save = || -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(&*device.secret())?;
        file.sync_all()
    };
    save().map_err(|error| error.to_string())?;
    let bytes = Zeroizing::new(fs::read(path).map_err(|error| error.to_string())?);
    let secret = <&[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| "Wrong device size")?;
    if Identity::from_secret(secret).peer_id() != device.peer_id() {
        return Err("Wrong saved device".into());
    }
    Ok(())
}

#[tokio::test]
async fn a_wire_invitation_releases_admission_before_retained_installation() {
    let address = link_node().await;
    let protocols = Arc::new(dyapp_p2p_net::Protocols::public());
    let mut sender = dyapp_client::Node::connect(address.clone(), Arc::clone(&protocols))
        .await
        .unwrap();
    let root = protocol_build_path("received-link-registry");
    let registry = Registry::new(root.clone(), Layout::Desktop);
    let (source, original, _) = registry.create(Storage::File, &Files).unwrap();
    let network = Network {
        name: "public".into(),
        seeds: vec![address.to_string()],
    };
    network::save(&source.dir, Settings::Active, &network).unwrap();
    let now = dyapp_client::now();
    let mut waiting = registry
        .begin_link(&source.id, &original.peer_id(), false, &Files, now)
        .unwrap();
    let code = waiting.code().unwrap();
    let owner = Identity::generate();
    code.send(&mut sender, &owner, b"received private state", now)
        .await
        .unwrap();
    let mut received = waiting
        .poll_with(&Files, now, |actual| async move {
            assert_eq!(actual, network);
            dyapp_client::Node::connect(address, protocols)
                .await
                .map_err(|error| error.to_string())
        })
        .await
        .unwrap()
        .expect("the completed send must be receivable");
    let guard = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("profiles.lock"))
        .unwrap();
    guard
        .try_lock()
        .expect("no shared receive operation survives completion");
    drop(guard);
    assert_eq!(waiting.code(), None);
    assert_eq!(registry.list().unwrap(), vec![source]);
    check_installation(&mut received, &root, &owner, code);
}

fn check_installation(
    received: &mut dyapp_client::identity_slots::ReceivedLink,
    root: &std::path::Path,
    owner: &Identity,
    code: Code,
) {
    let (slot, installed, _) = received.install_with(&Files, &persist_device).unwrap();
    assert_eq!(installed.peer_id(), owner.peer_id());
    let store = Store::open(&slot.dir.join("client.db"), installed).unwrap();
    assert_eq!(
        store.link_pending().unwrap().unwrap(),
        b"received private state"
    );
    let bytes = Zeroizing::new(fs::read(slot.dir.join("device.key")).unwrap());
    let secret = <&[u8; 32]>::try_from(bytes.as_slice()).unwrap();
    assert_eq!(
        dyapp_client::mailbox(&Identity::from_secret(secret).public_key()),
        code.mailbox
    );
    assert_eq!(
        dyapp_client::identity_slots::active(root, Layout::Desktop),
        dyapp_client::identity_slots::Active::Selected(slot.id.clone())
    );
}
