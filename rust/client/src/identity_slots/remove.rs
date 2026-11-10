use super::{
    active, keys, markers, owned, write, Active, KeyReader, Layout, Registry, Slot, State, Storage,
    LEGACY,
};
use std::fs;
use std::path::Path;

/// Platform credential role; core erases the identity last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Credential {
    /// MLS sealing secret.
    Seal,
    /// Device mailbox identity.
    Device,
    /// Identity secret or the mobile wrapping key protecting all local key blobs.
    Identity,
}

const ROLES: [Credential; 3] = [Credential::Seal, Credential::Device, Credential::Identity];

/// Only platform credential access; lifecycle, ordering and file removal stay in core.
pub trait KeyRemover: KeyReader {
    /// Test presence of this exact slot credential, including after erasure.
    ///
    /// # Errors
    /// Returns access failures, never a fabricated absence.
    fn credential_present(&self, slot: &Slot, role: Credential) -> Result<bool, String>;

    /// Erase this exact credential. Missing entries may be treated as already erased.
    ///
    /// # Errors
    /// Returns platform erasure failures.
    fn erase_credential(&self, slot: &Slot, role: Credential) -> Result<(), String>;
}

/// Completed visible removal, with independent persistence confirmation.
#[derive(Debug, PartialEq, Eq)]
pub struct RemovalOutcome {
    /// None when synchronization completed; otherwise removal is already visibly complete.
    pub durability_detail: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Prepared,
    Marked,
    SelectionCleared,
    CredentialsErased,
    Quarantined,
    DataCleared,
    Committed,
}

impl Registry {
    /// Remove an explicitly confirmed identity and only its validated owned files.
    /// Existing keys must match the supplied Peer ID exactly. A missing key permits
    /// cleanup only after credential absence is confirmed; no key is ever replaced.
    /// Hold no operation lease while calling this exclusive mutation.
    ///
    /// # Errors
    /// Returns unsafe metadata, confirmation, credential or cleanup failures.
    /// After mutation starts, failures retain discoverable deletion recovery state.
    pub fn remove(
        &self,
        id: &str,
        confirmed_peer: Option<&str>,
        keys: &dyn KeyRemover,
    ) -> Result<RemovalOutcome, String> {
        self.remove_with(id, confirmed_peer, keys, |_| Ok(()))
    }

    pub(super) fn remove_with(
        &self,
        id: &str,
        confirmed_peer: Option<&str>,
        keys: &dyn KeyRemover,
        mut checkpoint: impl FnMut(Stage) -> Result<(), String>,
    ) -> Result<RemovalOutcome, String> {
        let _guard = self.lock()?;
        let mut slot = super::list(&self.root, self.layout)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|slot| slot.id == id)
            .ok_or("Identity slot is missing")?;
        let verified = confirm(&slot, confirmed_peer, keys)?;
        owned::validate(&slot.dir).map_err(|error| error.to_string())?;
        checkpoint(Stage::Prepared)?;
        self.mark_removal(&slot)?;
        checkpoint(Stage::Marked)?;
        let mut detail = self.clear_selection(&slot)?;
        checkpoint(Stage::SelectionCleared)?;
        erase(&slot, keys)?;
        if verified && keys::load(&slot, keys)?.is_some() {
            return Err("The erased identity key still opens".into());
        }
        checkpoint(Stage::CredentialsErased)?;
        self.quarantine(&mut slot)?;
        checkpoint(Stage::Quarantined)?;
        owned::clear(&slot.dir).map_err(|error| error.to_string())?;
        checkpoint(Stage::DataCleared)?;
        remove_directory(&slot.dir)?;
        if self.layout == Layout::Ios {
            fs::remove_file(self.tombstone(&slot)).map_err(|error| error.to_string())?;
        }
        if let Err(error) = checkpoint(Stage::Committed)
            .and_then(|()| write::selection_sync(&self.root.join("profiles")))
        {
            detail.get_or_insert(error);
        }
        Ok(RemovalOutcome {
            durability_detail: detail,
        })
    }

    fn tombstone(&self, slot: &Slot) -> std::path::PathBuf {
        self.root
            .join("profiles")
            .join(format!("{}.deleting", slot.id))
    }

    fn mark_removal(&self, slot: &Slot) -> Result<(), String> {
        if self.layout == Layout::Ios {
            let marker = self.tombstone(slot);
            match markers::read(&marker)
                .map_err(|error| error.to_string())?
                .as_deref()
            {
                Some("deleting\n") => {}
                None => write::fresh(&marker, b"deleting\n")?,
                Some(_) => return Err("Invalid deletion marker".into()),
            }
        } else if slot.state != State::Deleting {
            write::replace(&slot.dir.join("state"), b"deleting\n")?;
        }
        Ok(())
    }

    fn clear_selection(&self, slot: &Slot) -> Result<Option<String>, String> {
        let pointer = active(&self.root, self.layout);
        let selected = matches!(&pointer, Active::Selected(id) if id == &slot.id);
        let path = self.root.join("active-profile");
        let status = if self.layout == Layout::Ios
            && (selected || matches!(pointer, Active::Absent | Active::Invalid))
        {
            Some(write::select(&path, b"none\n")?)
        } else if selected {
            fs::remove_file(&path).map_err(|error| error.to_string())?;
            Some(match write::selection_sync(&self.root) {
                Ok(()) => super::SelectionDurability::Synced,
                Err(detail) => super::SelectionDurability::Unconfirmed { detail },
            })
        } else {
            None
        };
        Ok(status.and_then(|status| match status {
            super::SelectionDurability::Synced => None,
            super::SelectionDurability::Unconfirmed { detail } => Some(detail),
        }))
    }

    fn quarantine(&self, slot: &mut Slot) -> Result<(), String> {
        if self.layout == Layout::Ios {
            return Ok(());
        }
        let hidden = self
            .root
            .join("profiles")
            .join(format!(".{}.removed", slot.id));
        if slot.dir == hidden {
            return Ok(());
        }
        match fs::symlink_metadata(&hidden) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => return Err("Identity cleanup path already exists".into()),
            Err(error) => return Err(error.to_string()),
        }
        fs::rename(&slot.dir, &hidden).map_err(|error| error.to_string())?;
        slot.dir = hidden;
        write::sync_directory(&self.root.join("profiles"))
    }
}

fn confirm(slot: &Slot, confirmed: Option<&str>, backend: &dyn KeyRemover) -> Result<bool, String> {
    if slot.id == LEGACY || slot.state == State::Broken {
        return Err("This identity cannot be removed".into());
    }
    if slot.storage.is_none() {
        return confirm_empty_storage(slot, backend).map(|()| false);
    }
    if orphan_oversize(slot, backend)? {
        return Ok(false);
    }
    match keys::load(slot, backend)? {
        Some(identity) if Some(identity.peer_id().as_str()) == confirmed => Ok(true),
        Some(_) => Err("The key does not match the confirmed Peer ID".into()),
        None if slot.storage != Some(Storage::File)
            && backend.credential_present(slot, Credential::Identity)? =>
        {
            Err("Identity credentials cannot be verified".into())
        }
        None => Ok(false),
    }
}

fn orphan_oversize(slot: &Slot, backend: &dyn KeyRemover) -> Result<bool, String> {
    if slot.storage != Some(Storage::Wrapped)
        || backend.credential_present(slot, Credential::Identity)?
    {
        return Ok(false);
    }
    match fs::symlink_metadata(slot.dir.join("identity.key")) {
        Ok(metadata) => Ok(metadata.is_file() && metadata.len() > keys::MAX_WRAPPED_BYTES as u64),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn confirm_empty_storage(slot: &Slot, backend: &dyn KeyRemover) -> Result<(), String> {
    for role in ROLES {
        if backend.credential_present(slot, role)? {
            return Err("Unknown storage still holds identity credentials".into());
        }
    }
    match fs::symlink_metadata(slot.dir.join("identity.key")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
        Ok(_) => Err("Unknown storage still holds an identity file".into()),
    }
}

fn erase(slot: &Slot, backend: &dyn KeyRemover) -> Result<(), String> {
    match slot.storage {
        Some(Storage::File) => {
            for name in ["seal.key", "device.key", "identity.key"] {
                let path = slot.dir.join(name);
                match fs::remove_file(&path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    result => result.map_err(|error| error.to_string())?,
                }
            }
        }
        Some(Storage::System) => {
            for role in ROLES {
                erase_role(slot, role, backend)?;
            }
        }
        Some(Storage::Wrapped) => erase_role(slot, Credential::Identity, backend)?,
        None => {}
    }
    Ok(())
}

fn erase_role(slot: &Slot, role: Credential, backend: &dyn KeyRemover) -> Result<(), String> {
    if backend.credential_present(slot, role)? {
        backend.erase_credential(slot, role)?;
    }
    if backend.credential_present(slot, role)? {
        return Err("The credential is still present after removal".into());
    }
    Ok(())
}

fn remove_directory(path: &Path) -> Result<(), String> {
    match fs::remove_dir(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result.map_err(|error| error.to_string()),
    }
}
