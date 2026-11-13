//! A linked device's identity slot, made from the owner's link record.

use super::create::{save, KeyWriter};
use super::{keys, write, KeyRemover, Registry, SelectionDurability, Slot, State};
use crate::link::Linked;
use crate::store::Store;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;
use std::fs;

impl Registry {
    /// Make a ready, selected slot holding `linked`, beside the slot `from` that waited for it.
    /// `device` stores this computer's device key in the new slot and checks it reopens.
    /// The waiting slot's network settings come along; the link state waits in the new database.
    /// With `wizard`, `from` was made only to link and goes if no profile draft was saved there.
    ///
    /// Until the final selection the previous one stays and an interrupted slot stays pending.
    ///
    /// # Errors
    /// Returns missing slots, storage, credential, verification, database or write failures.
    pub fn link<K: KeyWriter + KeyRemover>(
        &self,
        from: &str,
        linked: &Linked,
        wizard: bool,
        keys: &K,
        device: &dyn Fn(&Slot) -> Result<(), String>,
    ) -> Result<(Slot, Identity, SelectionDurability), String> {
        let _guard = self.lock()?;
        let old = super::list(&self.root, self.layout)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|slot| slot.id == from)
            .ok_or("Unknown identity")?;
        let storage = old.storage.ok_or("Unknown identity storage mode")?;
        let mut slot = self.reserve(storage, keys)?;
        save(&slot, storage, &linked.identity, keys)?;
        let reopened = keys::load(&slot, keys)?.ok_or("Saved identity is missing")?;
        if reopened.peer_id() != linked.identity.peer_id() {
            return Err("Saved identity does not reopen".into());
        }
        device(&slot)?;
        for which in [Settings::Active, Settings::Draft] {
            if let Some(settings) = network::load(&old.dir, which).map_err(|e| e.to_string())? {
                network::save(&slot.dir, which, &settings).map_err(|e| e.to_string())?;
            }
        }
        Store::open(&slot.dir.join("client.db"), &linked.identity)
            .and_then(|store| store.set_link_pending(&linked.state))
            .map_err(|error| error.to_string())?;
        write::replace(&slot.dir.join("state"), b"ready\n")?;
        slot.state = State::Ready;
        let durability = write::select(
            &self.root.join("active-profile"),
            format!("{}\n", slot.id).as_bytes(),
        )?;
        if wizard {
            // The link already succeeded; a failed cleanup stays visible as a deleting slot.
            if let Ok(Some(peer)) = unused(&old, keys) {
                let _ = self.remove_locked(&old.id, Some(&peer), keys, |_| Ok(()));
            }
        }
        Ok((slot, reopened, durability))
    }
}

/// The Peer ID of a ready slot with no saved profile draft.
fn unused(slot: &Slot, keys: &dyn KeyRemover) -> Result<Option<String>, String> {
    if slot.state != State::Ready {
        return Ok(None);
    }
    let identity = keys::load(slot, keys)?.ok_or("Identity key is missing")?;
    let database = slot.dir.join("client.db");
    if fs::symlink_metadata(&database).is_ok()
        && Store::open(&database, &identity)
            .and_then(|store| store.profile_draft())
            .map_err(|error| error.to_string())?
            .is_some()
    {
        return Ok(None);
    }
    Ok(Some(identity.peer_id()))
}
