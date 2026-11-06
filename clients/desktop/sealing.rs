//! Independent per-slot MLS sealing secret.

use crate::slots::{self, Slot};
use crate::{load_key, save_key};
use dyapp_identity::Identity;
use std::path::Path;

/// Independent per-slot MLS seal secret. A missing secret cannot reset existing MLS state.
pub(crate) fn sealing_key(
    root: &Path,
    dir: &Path,
    store: &dyapp_client::store::Store,
) -> Result<zeroize::Zeroizing<[u8; 32]>, String> {
    let slot = slots::list(root)?
        .into_iter()
        .find(|slot| slot.dir == dir && slot.state == slots::State::Ready)
        .ok_or("Unknown identity")?;
    let saved = load_key(&slot, &slot.seal_path(), &slot.seal_name())?;
    if saved.is_none()
        && (store.sealed_device().map_err(|e| e.to_string())?.is_some()
            || store.has_sticker_packs().map_err(|e| e.to_string())?)
    {
        return Err("Identity sealing key is missing or state is unreadable".into());
    }
    let secret = match saved {
        Some(secret) => secret,
        None => slots::device_key(
            root,
            dir,
            &|slot: &Slot| load_key(slot, &slot.seal_path(), &slot.seal_name()),
            &|slot: &Slot, key: &Identity| {
                save_key(slot, &slot.seal_path(), &slot.seal_name(), key)
            },
        )?,
    };
    Ok(zeroize::Zeroizing::new(*secret.secret()))
}

#[cfg(test)]
mod tests {
    use super::sealing_key;
    use crate::{device_key, load, save, slots};
    use dyapp_client::session::Session;
    use dyapp_client::store::Store;
    use dyapp_identity::Identity;

    #[test]
    fn saved_pack_files_prevent_key_regeneration_before_mls() {
        let root = std::env::current_dir().unwrap().join("build").join(format!(
            "dyapp-pack-seal-test-{}",
            Identity::generate().peer_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (slot, owner) = slots::create(&root, "file", &save, &load).unwrap();
        let store = Store::open(&slot.dir.join("client.db"), &owner).unwrap();
        let packs = slot.dir.join("sticker-packs");
        std::fs::create_dir(&packs).unwrap();
        std::fs::write(packs.join(".interrupted.pending"), b"preserve").unwrap();
        assert!(sealing_key(&root, &slot.dir, &store).is_err());
        assert!(!slot.seal_path().exists());
        assert_eq!(
            std::fs::read(packs.join(".interrupted.pending")).unwrap(),
            b"preserve"
        );
    }

    #[test]
    fn per_slot_seal_secret_persists_and_missing_key_refuses_existing_state() {
        let root = std::env::temp_dir().join(format!(
            "dyapp-seal-test-{}",
            Identity::generate().peer_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (slot, owner) = slots::create(&root, "file", &save, &load).unwrap();
        let device = device_key(&root, &slot.dir, &owner).unwrap();
        let path = slot.dir.join("client.db");
        let store = Store::open(&path, &owner).unwrap();
        let seal = sealing_key(&root, &slot.dir, &store).unwrap();
        let session = Session::open_or_create(store, &owner, &device, &seal).unwrap();
        let certificate = session.certificate().unwrap();
        drop(session);

        let store = Store::open(&path, &owner).unwrap();
        assert!(slot.seal_path().is_file());
        assert_eq!(
            Session::open(store, &seal).unwrap().certificate().unwrap(),
            certificate
        );
        let reopened = Store::open(&path, &owner).unwrap();
        assert_eq!(*sealing_key(&root, &slot.dir, &reopened).unwrap(), *seal);

        let (other, other_owner) = slots::create(&root, "file", &save, &load).unwrap();
        let other_store = Store::open(&other.dir.join("client.db"), &other_owner).unwrap();
        Session::open_or_create(other_store, &other_owner, &device, &[8; 32]).unwrap();
        let other_store = Store::open(&other.dir.join("client.db"), &other_owner).unwrap();
        assert!(device_key(&root, &other.dir, &other_owner).is_err());
        assert!(!other.device_path().exists());
        assert!(sealing_key(&root, &other.dir, &other_store).is_err());
        assert!(!other.seal_path().exists());
    }
}
