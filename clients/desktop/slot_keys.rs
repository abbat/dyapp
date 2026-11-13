//! The desktop's system key store behind the core identity registry. File-mode keys live in the
//! slot folder, where the core reads and writes them itself.

use dyapp_client::identity_slots::{
    self as core, Credential, KeyReader, KeyRemover, KeyWriter, Storage,
};
use dyapp_identity::Identity;

use crate::slots::{self, Slot};

/// The desktop view of a core slot, for the desktop's key names and paths.
pub(crate) fn local(slot: &core::Slot) -> Slot {
    Slot {
        id: slot.id.clone(),
        dir: slot.dir.clone(),
        mode: match slot.storage {
            Some(Storage::System) => Some("system"),
            Some(Storage::File) => Some("file"),
            Some(Storage::Wrapped) | None => None,
        },
        state: match slot.state {
            core::State::Pending => slots::State::Pending,
            core::State::Ready => slots::State::Ready,
            core::State::Deleting => slots::State::Deleting,
            core::State::Broken => slots::State::Broken,
        },
    }
}

fn name(slot: &core::Slot, role: Credential) -> String {
    let slot = local(slot);
    match role {
        Credential::Identity => slot.system_name(),
        Credential::Device => slot.device_name(),
        Credential::Seal => slot.seal_name(),
    }
}

/// Secret Service or Credential Manager entries of the desktop slots.
pub(crate) struct Keys;

impl KeyReader for Keys {
    fn contains(&self, slot: &core::Slot) -> Result<bool, String> {
        if slot.storage != Some(Storage::System) {
            return Ok(false);
        }
        for role in [Credential::Identity, Credential::Device, Credential::Seal] {
            if self.credential_present(slot, role)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn open(&self, slot: &core::Slot, wrapped: Option<&[u8]>) -> Result<Option<Identity>, String> {
        if wrapped.is_some() {
            return Err("Wrapped keys are not used on the desktop".into());
        }
        crate::system_store::load_slot(&name(slot, Credential::Identity))
    }
}

impl KeyWriter for Keys {
    fn store(&self, slot: &core::Slot, identity: &Identity) -> Result<Option<Vec<u8>>, String> {
        crate::system_store::save_slot(&name(slot, Credential::Identity), identity).map(|()| None)
    }
}

impl KeyRemover for Keys {
    fn credential_present(&self, slot: &core::Slot, role: Credential) -> Result<bool, String> {
        crate::system_store::load_slot(&name(slot, role)).map(|key| key.is_some())
    }

    fn erase_credential(&self, slot: &core::Slot, role: Credential) -> Result<(), String> {
        crate::system_store::delete_slot(&name(slot, role))
    }
}
