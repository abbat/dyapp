//! Desktop identity views and remaining lifecycle adapters. Inventory is delegated to the shared
//! core registry; see docs/development/desktop-identity-storage.md.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use dyapp_client::identity_slots::{self as core, Layout, RecoveryReason, Registry, Resolution};
use dyapp_identity::Identity;

use crate::slot_keys;

/// The virtual slot of the root `identity.key` / `identity-storage` installation.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub const LEGACY: &str = "legacy-primary";

/// Files belonging to the slot that a confirmed removal may erase.
const REMOVABLE: [&str; 18] = [
    "device.key",
    "seal.key",
    "storage-mode",
    "state",
    "state.tmp",
    "network",
    "network.tmp",
    "network-draft",
    "network-draft.tmp",
    "client.db",
    "client.db-wal",
    "client.db-shm",
    "client.db-journal",
    "node.json",
    "node.json.tmp",
    // Identity-local photo originals and sealed sticker packs.
    "originals",
    "sticker-packs",
    // The node's key and stores.
    "node",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub enum State {
    Pending,
    Ready,
    Deleting,
    /// Unreadable or tampered metadata: shown for recovery, never opened or overwritten.
    Broken,
}

#[derive(Debug)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Slot {
    pub id: String,
    pub dir: PathBuf,
    /// `system` or `file`; `None` when the mode marker is missing or invalid.
    pub mode: Option<&'static str>,
    pub state: State,
}

impl Slot {
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn key_path(&self) -> PathBuf {
        self.dir.join("identity.key")
    }

    /// The Secret Service or Credential Manager slot name.
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn system_name(&self) -> String {
        if self.id == LEGACY {
            "primary".into()
        } else {
            format!("profile:{}", self.id)
        }
    }

    /// Where the `file` mode keeps this computer's device mailbox key; see [`device_key`].
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn device_path(&self) -> PathBuf {
        self.dir.join("device.key")
    }

    /// The `system` mode's slot name for the device mailbox key.
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn device_name(&self) -> String {
        format!("{}:device", self.system_name())
    }

    /// Independent secret that seals this slot's MLS Device snapshot.
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn seal_path(&self) -> PathBuf {
        self.dir.join("seal.key")
    }

    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn seal_name(&self) -> String {
        format!("{}:seal", self.system_name())
    }
}

fn is_uuid(name: &str) -> bool {
    name.len() == 36
        && name.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => matches!(c, '0'..='9' | 'a'..='f'),
        })
}

fn new_uuid() -> Result<String, String> {
    // A throwaway key gives 16 OS-random bytes without another crate; it is never an identity.
    let bytes = Identity::try_generate()
        .map_err(|e| e.to_string())?
        .public_key();
    let hex = dyapp_identity::hex(&bytes[..16]);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

/// Reads a short marker file; `Ok(None)` if absent, `Err` if it is not a regular file.
fn read_marker(path: &Path) -> Result<Option<String>, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(metadata) if !metadata.is_file() => return Err("Marker is not a regular file".into()),
        Ok(_) => {}
    }
    let mut value = String::new();
    fs::File::open(path)
        .and_then(|file| file.take(64).read_to_string(&mut value))
        .map_err(|e| e.to_string())?;
    Ok(Some(value))
}

fn state_name(state: State) -> &'static str {
    match state {
        State::Pending => "pending\n",
        State::Ready => "ready\n",
        State::Deleting => "deleting\n",
        State::Broken => unreachable!("a broken state is never written"),
    }
}

fn write_new(path: &Path, value: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(value.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())
}

/// Replaces `path` by a flushed temporary file and a same-directory rename.
fn write_atomic(path: &Path, value: &str) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    let _ = fs::remove_file(&temp);
    write_new(&temp, value)?;
    fs::rename(&temp, path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        fs::File::open(dir)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

impl From<&dyapp_client::identity_slots::Slot> for Slot {
    fn from(slot: &dyapp_client::identity_slots::Slot) -> Self {
        use dyapp_client::identity_slots as core;
        Self {
            id: slot.id.clone(),
            dir: slot.dir.clone(),
            mode: match slot.storage {
                Some(core::Storage::System) => Some("system"),
                Some(core::Storage::File) => Some("file"),
                Some(core::Storage::Wrapped) | None => None,
            },
            state: match slot.state {
                core::State::Pending => State::Pending,
                core::State::Ready => State::Ready,
                core::State::Deleting => State::Deleting,
                core::State::Broken => State::Broken,
            },
        }
    }
}

fn registry(root: &Path) -> Registry {
    Registry::new(root.to_path_buf(), Layout::Desktop)
}

/// Core inventory includes interrupted cleanup and refuses ambiguous duplicate paths.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn list(root: &Path) -> Result<Vec<Slot>, String> {
    registry(root)
        .list()
        .map(|slots| slots.iter().map(Slot::from).collect())
}

fn recovery_reason(reason: RecoveryReason) -> &'static str {
    match reason {
        RecoveryReason::Missing => "missing",
        RecoveryReason::Unavailable => "unavailable",
        RecoveryReason::Incomplete => "incomplete",
        RecoveryReason::Deleting => "deleting",
        RecoveryReason::Damaged | RecoveryReason::InvalidPointer => "damaged",
    }
}

fn project(slot: Option<Slot>, resolution: Resolution) -> Result<Opened, String> {
    Ok(match resolution {
        Resolution::Setup => Opened::Setup,
        Resolution::Picker(_) => Opened::Picker,
        Resolution::Ready(slot, identity) => Opened::Ready(Slot::from(&slot), *identity),
        Resolution::Recovery {
            reason, peer_id, ..
        } => Opened::Recovery(
            slot.ok_or("Identity recovery slot is missing")?,
            recovery_reason(reason),
            peer_id,
        ),
    })
}

/// Inspect one inventoried candidate through the core without changing selection.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn inspect(root: &Path, slot: Slot) -> Result<Opened, String> {
    let resolution = registry(root).inspect(&slot.id, &slot_keys::Keys)?;
    project(Some(slot), resolution)
}

/// Resolve the persisted selection through the core, retaining the desktop recovery view.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn resolve(root: &Path) -> Result<Opened, String> {
    let registry = registry(root);
    let resolution = registry.resolve(&slot_keys::Keys)?;
    if let Resolution::Recovery {
        id,
        reason,
        peer_id,
    } = resolution
    {
        let slot = registry
            .list()?
            .iter()
            .find(|slot| Some(&slot.id) == id.as_ref())
            .map_or_else(
                || Slot {
                    id: id.unwrap_or_else(|| LEGACY.into()),
                    dir: root.into(),
                    mode: None,
                    state: State::Broken,
                },
                Slot::from,
            );
        return Ok(Opened::Recovery(slot, recovery_reason(reason), peer_id));
    }
    project(None, resolution)
}

/// Reserves a fresh slot for a key in `mode`, written as `pending` before any key exists.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn reserve(root: &Path, mode: &'static str) -> Result<Slot, String> {
    let profiles = root.join("profiles");
    fs::create_dir_all(&profiles).map_err(|e| e.to_string())?;
    for _ in 0..8 {
        let id = new_uuid()?;
        let dir = profiles.join(&id);
        match fs::create_dir(&dir) {
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            result => result.map_err(|e| e.to_string())?,
        }
        write_new(&dir.join("storage-mode"), &format!("{mode}\n"))?;
        write_new(&dir.join("state"), state_name(State::Pending))?;
        return Ok(Slot {
            id,
            dir,
            mode: Some(mode),
            state: State::Pending,
        });
    }
    Err("Could not reserve a new identity slot".into())
}

/// Moves a profile slot to `state`; the legacy slot has no state file.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn set_state(slot: &mut Slot, state: State) -> Result<(), String> {
    if slot.id == LEGACY || slot.state == State::Broken {
        return Err("Slot state cannot change".into());
    }
    write_atomic(&slot.dir.join("state"), state_name(state))?;
    slot.state = state;
    Ok(())
}

/// The slot named by `active-profile`, if the pointer is valid.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn active(root: &Path) -> Option<String> {
    let value = read_marker(&root.join("active-profile")).ok()??;
    let id = value.strip_suffix('\n')?;
    (id == LEGACY || is_uuid(id)).then(|| id.to_owned())
}

#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn set_active(root: &Path, id: &str) -> Result<(), String> {
    write_atomic(&root.join("active-profile"), &format!("{id}\n"))
}

/// Reads a slot's key: `None` when its backend confirms there is no key.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub type Load<'a> = &'a dyn Fn(&Slot) -> Result<Option<Identity>, String>;

#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub enum Opened {
    /// No slot yet: offer the storage choice.
    Setup,
    Ready(Slot, Identity),
    /// Several slots and no valid pointer.
    Picker,
    /// The slot to open cannot be used as it is; the Peer ID is known when its key opens.
    Recovery(Slot, &'static str, Option<String>),
}

/// Opens one listed slot; a key that cannot be read is "unavailable", so the window can retry.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn open_slot(slot: Slot, load: Load<'_>) -> Result<Opened, String> {
    let reason = match slot.state {
        State::Ready => match load(&slot) {
            Ok(Some(identity)) => return Ok(Opened::Ready(slot, identity)),
            Ok(None) => "missing",
            Err(_) => "unavailable",
        },
        State::Pending | State::Deleting => {
            let reason = if slot.state == State::Pending {
                "incomplete"
            } else {
                "deleting"
            };
            let peer_id = load(&slot)?.map(|identity| identity.peer_id());
            return Ok(Opened::Recovery(slot, reason, peer_id));
        }
        State::Broken => "damaged",
    };
    Ok(Opened::Recovery(slot, reason, None))
}

/// Holds `profiles.lock`, which serializes slot changes across app instances, until dropped.
fn lock(root: &Path) -> Result<fs::File, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("profiles.lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    Ok(lock)
}

/// Creates an identity in a fresh slot and makes it active; see [`adopt`].
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn create(
    root: &Path,
    mode: &'static str,
    save: &dyn Fn(&Slot, &Identity) -> Result<(), String>,
    load: Load<'_>,
) -> Result<(Slot, Identity), String> {
    let identity = Identity::try_generate().map_err(|e| e.to_string())?;
    let slot = adopt(root, mode, &identity, save, load, &|_| Ok(()))?;
    Ok((slot, identity))
}

/// Puts `identity` in a fresh slot and makes it active. The slot is marked ready only after its key
/// reopens with the same Peer ID and `fill` has added what else it needs, so a failure or a crash
/// leaves an incomplete slot and the active one in place. `fill` runs under the slot lock.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn adopt(
    root: &Path,
    mode: &'static str,
    identity: &Identity,
    save: &dyn Fn(&Slot, &Identity) -> Result<(), String>,
    load: Load<'_>,
    fill: &dyn Fn(&Slot) -> Result<(), String>,
) -> Result<Slot, String> {
    let _lock = lock(root)?;
    let mut slot = reserve(root, mode)?;
    save(&slot, identity)?;
    match load(&slot)? {
        Some(saved) if saved.peer_id() == identity.peer_id() => {}
        _ => return Err("Saved identity does not reopen".into()),
    }
    fill(&slot)?;
    set_state(&mut slot, State::Ready)?;
    set_active(root, &slot.id)?;
    Ok(slot)
}

/// The device mailbox key of the ready slot in `dir`, made on first use. It is random and kept
/// beside the identity key, so a copy of the identity on another computer becomes a new device.
/// The slot lock keeps two callers from making two keys.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn device_key(
    root: &Path,
    dir: &Path,
    load: Load<'_>,
    save: &dyn Fn(&Slot, &Identity) -> Result<(), String>,
) -> Result<Identity, String> {
    let _lock = lock(root)?;
    let slot = core::list(root, Layout::Desktop)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|slot| Slot::from(&slot))
        .find(|slot| slot.dir == dir && slot.state == State::Ready)
        .ok_or("Unknown identity")?;
    if let Some(key) = load(&slot)? {
        return Ok(key);
    }
    let key = Identity::try_generate().map_err(|e| e.to_string())?;
    save(&slot, &key)?;
    match load(&slot)? {
        Some(saved) if saved.public_key() == key.public_key() => Ok(key),
        _ => Err("Saved device key does not reopen".into()),
    }
}

/// Removes slot `id`. A key that still exists must yield `peer_id`, the Peer ID the user
/// confirmed. The slot is marked `deleting` before `erase` and its files go only once `load`
/// confirms the key is absent, so any failure keeps a slot that a retry can finish. The legacy
/// slot and damaged slots are refused: they need a migration or hold files we cannot interpret.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn remove(
    root: &Path,
    id: &str,
    peer_id: Option<&str>,
    load: Load<'_>,
    erase: &dyn Fn(&Slot) -> Result<(), String>,
) -> Result<(), String> {
    let _lock = lock(root)?;
    let mut slot = core::list(root, Layout::Desktop)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|slot| Slot::from(&slot))
        .find(|slot| slot.id == id)
        .ok_or("Unknown identity")?;
    if slot.id == LEGACY || slot.state == State::Broken {
        return Err("This identity cannot be removed".into());
    }
    if let Some(identity) = load(&slot)? {
        if Some(identity.peer_id().as_str()) != peer_id {
            return Err("The key does not match the confirmed Peer ID".into());
        }
    }
    if slot.state != State::Deleting {
        set_state(&mut slot, State::Deleting)?;
    }
    erase(&slot)?;
    if load(&slot)?.is_some() {
        return Err("The key is still present after removal".into());
    }
    for entry in fs::read_dir(&slot.dir).map_err(|e| e.to_string())? {
        let name = entry.map_err(|e| e.to_string())?.file_name();
        if !name.to_str().is_some_and(|name| REMOVABLE.contains(&name)) {
            return Err("The identity folder holds other files".into());
        }
    }
    // Core inventory keeps this quarantine discoverable if cleanup is interrupted.
    let removed = slot.dir.with_file_name(format!(".{id}.removed"));
    fs::rename(&slot.dir, &removed).map_err(|e| e.to_string())?;
    fs::remove_dir_all(&removed).map_err(|e| e.to_string())?;
    if active(root).as_deref() == Some(id) {
        fs::remove_file(root.join("active-profile")).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        // Parallel Windows tests can read the same clock value.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "dyapp-slots-test-{}-{nonce}-{n}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        root
    }

    fn describe_opened(opened: &Opened) -> String {
        match opened {
            Opened::Setup => "setup".to_owned(),
            Opened::Ready(slot, identity) => format!("ready {} {}", slot.id, identity.peer_id()),
            Opened::Picker => "picker".to_owned(),
            Opened::Recovery(slot, reason, peer) => format!("{reason} {} {peer:?}", slot.id),
        }
    }

    #[test]
    fn core_resolution_preserves_desktop_states_without_creating_a_key() {
        let root = temp_root();
        let state = |root: &Path| describe_opened(&resolve(root).unwrap());
        assert_eq!(state(&root), "setup");
        fs::write(root.join("active-profile"), "../escape\n").unwrap();
        assert_eq!(state(&root), "damaged legacy-primary None");
        fs::remove_file(root.join("active-profile")).unwrap();

        let mut slot = reserve(&root, "file").unwrap();
        assert_eq!(state(&root), format!("incomplete {} None", slot.id));
        let identity = Identity::try_generate().unwrap();
        crate::save(&slot, &identity).unwrap();
        let peer = identity.peer_id();
        assert_eq!(
            state(&root),
            format!("incomplete {} Some({peer:?})", slot.id)
        );
        set_state(&mut slot, State::Ready).unwrap();
        assert_eq!(state(&root), format!("ready {} {peer}", slot.id));
        assert_eq!(list(&root).unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_device_key_is_made_once_per_ready_slot() {
        use std::cell::RefCell;
        use std::collections::HashMap;
        let root = temp_root();
        let keys = RefCell::new(HashMap::new());
        let save = |name: String, key: &Identity| {
            keys.borrow_mut().insert(name, *key.secret());
            Ok::<_, String>(())
        };
        let load =
            |name: String| Ok::<_, String>(keys.borrow().get(&name).map(Identity::from_secret));
        let (slot, alice) = create(
            &root,
            "system",
            &|slot: &Slot, key: &Identity| save(slot.system_name(), key),
            &|slot: &Slot| load(slot.system_name()),
        )
        .unwrap();
        let load_device = |slot: &Slot| load(slot.device_name());
        let save_device = |slot: &Slot, key: &Identity| save(slot.device_name(), key);
        let device = device_key(&root, &slot.dir, &load_device, &save_device).unwrap();
        assert_ne!(device.public_key(), alice.public_key());
        let again = device_key(&root, &slot.dir, &load_device, &save_device).unwrap();
        assert_eq!(again.public_key(), device.public_key());
        // A save that does not reopen and a folder that is no ready slot are refused.
        keys.borrow_mut().remove(&slot.device_name());
        let lost = |_: &Slot, _: &Identity| Ok::<_, String>(());
        assert!(device_key(&root, &slot.dir, &load_device, &lost).is_err());
        let pending = reserve(&root, "file").unwrap();
        assert!(device_key(&root, &pending.dir, &load_device, &save_device).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
