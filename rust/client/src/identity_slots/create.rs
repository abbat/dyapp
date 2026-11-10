use super::{is_uuid, keys, write, KeyReader, Layout, Registry, Slot, State, Storage};
use dyapp_identity::Identity;
use std::fs;

/// Platform credential creation; core owns every identity-file write.
pub trait KeyWriter: KeyReader {
    /// Store a fresh system credential (None) or return newly wrapped bytes (Some).
    /// Must refuse an existing credential rather than replacing it.
    ///
    /// # Errors
    /// Returns platform storage/wrapping failures; the slot remains incomplete.
    fn store(&self, slot: &Slot, identity: &Identity) -> Result<Option<Vec<u8>>, String>;
}

impl Registry {
    /// Explicitly create a new identity and select it only after verified reopening.
    ///
    /// # Errors
    /// Returns invalid storage choices, reservation, credential, verification or write failures.
    pub fn create(
        &self,
        storage: Storage,
        keys: &dyn KeyWriter,
    ) -> Result<(Slot, Identity, super::SelectionDurability), String> {
        let _guard = self.lock()?;
        if (self.layout == Layout::Desktop) == (storage == Storage::Wrapped) {
            return Err("Storage does not match the registry layout".into());
        }
        let mut slot = self.reserve(storage, keys)?;
        let identity = Identity::generate();
        save(&slot, storage, &identity, keys)?;
        let reopened = keys::load(&slot, keys)?.ok_or("Saved identity is missing")?;
        if reopened.peer_id() != identity.peer_id() {
            return Err("Saved identity does not reopen".into());
        }
        write::replace(&slot.dir.join("state"), b"ready\n")?;
        slot.state = State::Ready;
        let durability = write::select(
            &self.root.join("active-profile"),
            format!("{}\n", slot.id).as_bytes(),
        )?;
        Ok((slot, identity, durability))
    }

    fn reserve(&self, storage: Storage, keys: &dyn KeyWriter) -> Result<Slot, String> {
        self.reserve_with(storage, keys, new_id)
    }

    pub(super) fn reserve_with(
        &self,
        storage: Storage,
        keys: &dyn KeyWriter,
        mut next_id: impl FnMut() -> Result<String, String>,
    ) -> Result<Slot, String> {
        let profiles = self.root.join("profiles");
        match fs::symlink_metadata(&profiles) {
            Ok(metadata) if !metadata.is_dir() => return Err("Profiles is not a directory".into()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&profiles).map_err(|error| error.to_string())?;
                write::sync_directory(&self.root)?;
            }
            Err(error) => return Err(error.to_string()),
        }
        for _ in 0..8 {
            let id = next_id()?;
            if !is_uuid(&id) {
                return Err("Invalid generated slot identifier".into());
            }
            let slot = Slot {
                dir: profiles.join(&id),
                id,
                state: State::Pending,
                storage: Some(storage),
            };
            if keys.contains(&slot)?
                || fs::symlink_metadata(profiles.join(format!("{}.deleting", slot.id))).is_ok()
                || fs::symlink_metadata(profiles.join(format!(".{}.removed", slot.id))).is_ok()
            {
                continue;
            }
            match fs::create_dir(&slot.dir) {
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                result => result.map_err(|error| error.to_string())?,
            }
            write::sync_directory(&profiles)?;
            if self.layout == Layout::Desktop {
                let mode = if storage == Storage::System {
                    b"system\n".as_slice()
                } else {
                    b"file\n"
                };
                write::fresh(&slot.dir.join("storage-mode"), mode)?;
            }
            write::fresh(&slot.dir.join("state"), b"pending\n")?;
            return Ok(slot);
        }
        Err("Could not reserve a new identity slot".into())
    }
}

fn save(
    slot: &Slot,
    storage: Storage,
    identity: &Identity,
    keys: &dyn KeyWriter,
) -> Result<(), String> {
    if storage == Storage::File {
        return write::fresh(&slot.dir.join("identity.key"), &*identity.secret());
    }
    match (storage, keys.store(slot, identity)?) {
        (Storage::System, None) => Ok(()),
        (Storage::Wrapped, Some(bytes)) if bytes.len() <= 4096 => {
            write::fresh(&slot.dir.join("identity.key"), &bytes)
        }
        _ => Err("Invalid platform key storage result".into()),
    }
}

fn new_id() -> Result<String, String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|error| error.to_string())?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = format!("{:032x}", u128::from_be_bytes(bytes));
    let id = format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    );
    debug_assert!(is_uuid(&id));
    Ok(id)
}
