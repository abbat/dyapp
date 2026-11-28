//! Desktop identity views and device-key helpers. Lifecycle is delegated to the shared core
//! registry; see docs/development/desktop-identity-storage.md.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use dyapp_client::identity_slots::{
    self as core, Layout, RecoveryReason, Registry, RemovalOutcome, Resolution,
    SelectionDurability, Storage,
};
use dyapp_identity::Identity;

use crate::slot_keys;

/// The virtual slot of the root `identity.key` / `identity-storage` installation.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub const LEGACY: &str = "legacy-primary";

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
    #[cfg(test)]
    #[expect(unreachable_pub, reason = "Retained test fixture visibility")]
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

/// The slot named by `active-profile`, if the pointer is valid.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn active(root: &Path) -> Option<String> {
    match core::active(root, Layout::Desktop) {
        core::Active::Selected(id) => Some(id),
        _ => None,
    }
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

/// Creates and verifies a fresh identity through the shared registry.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn create(
    root: &Path,
    mode: &'static str,
) -> Result<(Slot, Identity, SelectionDurability), String> {
    let storage = match mode {
        "system" => Storage::System,
        "file" => Storage::File,
        _ => return Err("Unknown identity storage mode".into()),
    };
    let (slot, identity, durability) = registry(root).create(storage, &slot_keys::Keys)?;
    Ok((Slot::from(&slot), identity, durability))
}

/// Selects an existing ready identity through the shared registry.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn select(root: &Path, id: &str) -> Result<(Opened, SelectionDurability), String> {
    let (slot, identity, durability) = registry(root).select(id, &slot_keys::Keys)?;
    Ok((Opened::Ready(Slot::from(&slot), *identity), durability))
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

/// Removes an explicitly confirmed identity through the shared registry.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn remove(root: &Path, id: &str, peer_id: Option<&str>) -> Result<RemovalOutcome, String> {
    registry(root).remove(id, peer_id, &slot_keys::Keys)
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

    fn pending(root: &Path) -> Slot {
        let id = "11111111-1111-1111-1111-111111111111";
        let dir = root.join("profiles").join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("storage-mode"), "file\n").unwrap();
        fs::write(dir.join("state"), "pending\n").unwrap();
        Slot {
            id: id.into(),
            dir,
            mode: Some("file"),
            state: State::Pending,
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

        let mut slot = pending(&root);
        assert_eq!(state(&root), format!("incomplete {} None", slot.id));
        let identity = Identity::try_generate().unwrap();
        crate::save(&slot, &identity).unwrap();
        let peer = identity.peer_id();
        assert_eq!(
            state(&root),
            format!("incomplete {} Some({peer:?})", slot.id)
        );
        fs::write(slot.dir.join("state"), "ready\n").unwrap();
        slot.state = State::Ready;
        assert_eq!(state(&root), format!("ready {} {peer}", slot.id));
        assert_eq!(list(&root).unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_committed_windows_selection_reopens_with_unconfirmed_durability() {
        let root = temp_root();
        let (first, _, _) = create(&root, "file").unwrap();
        let (second, _, _) = create(&root, "file").unwrap();
        assert_ne!(first.id, second.id);
        let (selected, durability) = select(&root, &first.id).unwrap();
        assert!(matches!(
            durability,
            SelectionDurability::Unconfirmed { .. }
        ));
        assert!(matches!(&selected, Opened::Ready(slot, _) if slot.id == first.id));
        assert_eq!(
            describe_opened(&resolve(&root).unwrap()),
            describe_opened(&selected)
        );
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
        let (slot, alice, _) = create(&root, "file").unwrap();
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
        let pending = pending(&root);
        assert!(device_key(&root, &pending.dir, &load_device, &save_device).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
