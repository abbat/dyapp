//! A linked device's identity slot, made from the owner's link record.

use super::create::{save, KeyWriter};
use super::{keys, write, KeyRemover, Registry, SelectionDurability, Slot, State};
use crate::link::Linked;
use crate::store::Store;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;
use std::fs;

/// Installation boundary reached before a link failed; never a rollback or lifecycle assertion.
#[derive(Debug, thiserror::Error)]
pub enum LinkFailure {
    /// No normal result is available. Refresh inventory and selection before recovery.
    #[error("{0}")]
    Unconfirmed(String),
    /// Installation did not start, so this attempt reserved no destination.
    #[error("{0}")]
    Admission(String),
    /// Reservation failed; a directory may exist, but no destination selector is confirmed.
    #[error("{0}")]
    Reservation(String),
    /// A destination was reserved. Re-read inventory rather than assuming it is still pending.
    #[error("{detail}")]
    Destination {
        /// Exact reserved slot identifier, not a filesystem path or an inferred inventory difference.
        slot_id: String,
        /// Original installation failure, without replacing it with a rollback claim.
        detail: String,
    },
}

/// A borrowed installation record keeps ownership and erasure with the receiving workflow.
#[derive(Clone, Copy)]
pub(super) struct LinkRecord<'a> {
    pub identity: &'a Identity,
    pub state: &'a [u8],
}

impl<'a> From<&'a Linked> for LinkRecord<'a> {
    fn from(linked: &'a Linked) -> Self {
        Self {
            identity: &linked.identity,
            state: &linked.state,
        }
    }
}

impl Registry {
    /// Make a ready, selected slot holding `linked`, beside the slot `from` that waited for it.
    /// `device` stores this computer's device key in the new slot and checks it reopens.
    /// The waiting slot's network settings come along; the link state waits in the new database.
    /// With `wizard`, `from` was made only to link and goes if no profile draft was saved there.
    ///
    /// Until final selection the previous one stays; a destination can become ready beforehand.
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
        self.install_link(&old, linked.into(), wizard, keys, device)
            .map_err(|error| error.to_string())
    }

    /// Install only while the invitation's source remains the selected ready owner.
    /// Release any shared identity operation before entering this exclusive mutation.
    /// Receiver cancellation/generation remains a separate admission requirement.
    ///
    /// # Errors
    /// Returns changed selection, lifecycle, owner, credential or link-installation failures.
    pub fn link_selected<K: KeyWriter + KeyRemover>(
        &self,
        from: &str,
        expected_peer: &str,
        linked: &Linked,
        wizard: bool,
        keys: &K,
        device: &dyn Fn(&Slot) -> Result<(), String>,
    ) -> Result<(Slot, Identity, SelectionDurability), String> {
        self.link_selected_with(from, expected_peer, linked.into(), wizard, keys, device)
            .map_err(|error| error.to_string())
    }

    /// Install a linked mobile identity and its device through existing slot credentials.
    /// Core verifies and persists the device before the slot becomes ready or selected.
    /// Release shared operations first; caller cancellation admission remains separate.
    ///
    /// # Errors
    /// Returns unsupported layout, source admission, wrapping or installation failures.
    pub fn link_selected_wrapped<K: KeyWriter + KeyRemover + super::ChatKeyStore>(
        &self,
        from: &str,
        expected_peer: &str,
        linked: &Linked,
        wizard: bool,
        keys: &K,
        device: &Identity,
    ) -> Result<(Slot, Identity, SelectionDurability), LinkFailure> {
        if self.layout == super::Layout::Desktop {
            return Err(LinkFailure::Admission(
                "Wrapped linking requires a mobile layout".into(),
            ));
        }
        self.link_selected_with(from, expected_peer, linked.into(), wizard, keys, &|slot| {
            super::chat_keys::persist_link_device(slot, device, keys)
        })
    }

    pub(super) fn link_selected_with<K: KeyWriter + KeyRemover>(
        &self,
        from: &str,
        expected_peer: &str,
        linked: LinkRecord<'_>,
        wizard: bool,
        keys: &K,
        device: &dyn Fn(&Slot) -> Result<(), String>,
    ) -> Result<(Slot, Identity, SelectionDurability), LinkFailure> {
        let _guard = self.lock().map_err(LinkFailure::Admission)?;
        let old = self
            .link_source(from, expected_peer, keys)
            .map_err(LinkFailure::Admission)?;
        self.install_link(&old, linked, wizard, keys, device)
    }

    pub(super) fn link_source(
        &self,
        from: &str,
        expected_peer: &str,
        keys: &dyn super::KeyReader,
    ) -> Result<Slot, String> {
        let super::Resolution::Ready(old, identity) =
            super::resolve(&self.root, self.layout, keys)?
        else {
            return Err("The link source is no longer ready".into());
        };
        if old.id != from || identity.peer_id() != expected_peer {
            return Err("The link source changed".into());
        }
        Ok(old)
    }

    fn install_link<K: KeyWriter + KeyRemover>(
        &self,
        old: &Slot,
        linked: LinkRecord<'_>,
        wizard: bool,
        keys: &K,
        device: &dyn Fn(&Slot) -> Result<(), String>,
    ) -> Result<(Slot, Identity, SelectionDurability), LinkFailure> {
        let storage = old
            .storage
            .ok_or_else(|| LinkFailure::Admission("Unknown identity storage mode".into()))?;
        let slot = self
            .reserve(storage, keys)
            .map_err(LinkFailure::Reservation)?;
        let slot_id = slot.id.clone();
        self.finish_link(old, slot, linked, wizard, keys, device)
            .map_err(|detail| LinkFailure::Destination { slot_id, detail })
    }

    fn finish_link<K: KeyWriter + KeyRemover>(
        &self,
        old: &Slot,
        mut slot: Slot,
        linked: LinkRecord<'_>,
        wizard: bool,
        keys: &K,
        device: &dyn Fn(&Slot) -> Result<(), String>,
    ) -> Result<(Slot, Identity, SelectionDurability), String> {
        let storage = slot.storage.ok_or("Unknown identity storage mode")?;
        save(&slot, storage, linked.identity, keys)?;
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
        Store::open(&slot.dir.join("client.db"), linked.identity)
            .and_then(|store| store.set_link_pending(linked.state))
            .map_err(|error| error.to_string())?;
        write::replace(&slot.dir.join("state"), b"ready\n")?;
        slot.state = State::Ready;
        let durability = write::select(
            &self.root.join("active-profile"),
            format!("{}\n", slot.id).as_bytes(),
        )?;
        if wizard {
            // The link already succeeded; a failed cleanup stays visible as a deleting slot.
            if let Ok(Some(peer)) = unused(old, keys) {
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
