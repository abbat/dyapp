//! Independent per-slot MLS sealing secret and migration from the legacy device-key derivation.

use std::path::Path;
use dyapp_identity::Identity;
use crate::slots::{self, Slot};
use crate::{load_key, profile, save_key};

/// Independent per-slot MLS seal secret. Persist it before rewrapping a legacy snapshot, so a
/// crash can retry the migration with either key. A missing new secret cannot reset old MLS state.
pub(crate) fn sealing_key(
    root: &Path,
    dir: &Path,
    store: &mut dyapp_client::store::Store,
    device: &Identity,
) -> Result<zeroize::Zeroizing<[u8; 32]>, String> {
    use dyapp_client::session::Session;
    let slot = slots::list(root)?
        .into_iter()
        .find(|slot| slot.dir == dir && slot.state == slots::State::Ready)
        .ok_or("Unknown identity")?;
    let saved = load_key(&slot, &slot.seal_path(), &slot.seal_name())?;
    let old = profile::legacy_seal_key(device);
    if saved.is_none()
        && store.sealed_device().map_err(|e| e.to_string())?.is_some()
        && !Session::seal_opens(store, &old).map_err(|e| e.to_string())?
    {
        return Err("MLS sealing key is missing or state is unreadable".into());
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
    let key = zeroize::Zeroizing::new(*secret.secret());
    Session::reseal(store, &old, &key).map_err(|e| e.to_string())?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::sealing_key;
    use crate::{device_key, load, save, slots};
    use dyapp_client::session::Session;
    use dyapp_client::store::Store;
    use dyapp_identity::Identity;

    #[test]
    fn per_slot_seal_secret_migrates_legacy_mls_state_without_reset() {
        let root = std::env::temp_dir().join(format!(
            "dyapp-seal-test-{}",
            Identity::generate().peer_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (slot, owner) = slots::create(&root, "file", &save, &load).unwrap();
        let device = device_key(&root, &slot.dir, &owner).unwrap();
        let legacy = super::profile::legacy_seal_key(&device);
        let path = slot.dir.join("client.db");
        let store = Store::open(&path, &owner).unwrap();
        let session = Session::open_or_create(store, &owner, &device, &legacy).unwrap();
        let certificate = session.certificate().unwrap();
        drop(session);

        let mut store = Store::open(&path, &owner).unwrap();
        let new = sealing_key(&root, &slot.dir, &mut store, &device).unwrap();
        assert_ne!(*new, *legacy);
        assert!(slot.seal_path().is_file());
        assert!(!Session::seal_opens(&store, &legacy).unwrap());
        assert_eq!(Session::open(store, &new).unwrap().certificate().unwrap(), certificate);
        let mut reopened = Store::open(&path, &owner).unwrap();
        assert_eq!(*sealing_key(&root, &slot.dir, &mut reopened, &device).unwrap(), *new);

        let (other, other_owner) = slots::create(&root, "file", &save, &load).unwrap();
        let mut other_store = Store::open(&other.dir.join("client.db"), &other_owner).unwrap();
        Session::open_or_create(other_store, &other_owner, &device, &[8; 32]).unwrap();
        other_store = Store::open(&other.dir.join("client.db"), &other_owner).unwrap();
        assert!(device_key(&root, &other.dir, &other_owner).is_err());
        assert!(!other.device_path().exists());
        assert!(sealing_key(&root, &other.dir, &mut other_store, &device).is_err());
        assert!(!other.seal_path().exists());
    }

}
