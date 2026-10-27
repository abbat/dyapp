//! Local identity slots: `profiles/<uuid>/` directories beside the legacy root key, discovered by
//! a directory scan rather than a registry; see docs/development/desktop-identity-storage.md.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use dyapp_identity::Identity;

/// The virtual slot of the root `identity.key` / `identity-storage` installation.
pub const LEGACY: &str = "legacy-primary";

/// Files belonging to the slot that a confirmed removal may erase.
const REMOVABLE: [&str; 14] = [
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
    // The node's key and stores.
    "node",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Pending,
    Ready,
    Deleting,
    /// Unreadable or tampered metadata: shown for recovery, never opened or overwritten.
    Broken,
}

#[derive(Debug)]
pub struct Slot {
    pub id: String,
    pub dir: PathBuf,
    /// `system` or `file`; `None` when the mode marker is missing or invalid.
    pub mode: Option<&'static str>,
    pub state: State,
}

impl Slot {
    pub fn key_path(&self) -> PathBuf {
        self.dir.join("identity.key")
    }

    /// The Secret Service or Credential Manager slot name.
    pub fn system_name(&self) -> String {
        if self.id == LEGACY {
            "primary".into()
        } else {
            format!("profile:{}", self.id)
        }
    }
}

fn is_uuid(name: &str) -> bool {
    name.len() == 36
        && name.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => matches!(c, '0'..='9' | 'a'..='f'),
        })
}

fn new_uuid() -> String {
    // A throwaway key gives 16 OS-random bytes without another crate; it is never an identity.
    let bytes = Identity::generate().public_key();
    let hex: String = bytes[..16].iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
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

fn parse_mode(value: &str) -> Option<&'static str> {
    match value {
        "system\n" => Some("system"),
        "file\n" => Some("file"),
        _ => None,
    }
}

fn parse_state(value: &str) -> State {
    match value {
        "pending\n" => State::Pending,
        "ready\n" => State::Ready,
        "deleting\n" => State::Deleting,
        _ => State::Broken,
    }
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

fn read_slot(id: String, dir: PathBuf) -> Slot {
    let mode = read_marker(&dir.join("storage-mode"))
        .ok()
        .flatten()
        .as_deref()
        .and_then(parse_mode);
    let state = match read_marker(&dir.join("state")) {
        Ok(Some(state)) if mode.is_some() => parse_state(&state),
        _ => State::Broken,
    };
    Slot {
        id,
        dir,
        mode,
        state,
    }
}

/// All local slots: the legacy one if its key or marker exists, then each UUID directory under
/// `profiles/`. Other names there are not ours and are skipped.
pub fn list(root: &Path) -> Result<Vec<Slot>, String> {
    let mut slots = Vec::new();
    let legacy_mode = read_marker(&root.join("identity-storage"));
    let legacy_key = fs::symlink_metadata(root.join("identity.key")).is_ok();
    match legacy_mode {
        Ok(None) if !legacy_key => {}
        Ok(mode) => {
            let mode = mode.as_deref().and_then(parse_mode);
            let state = if mode.is_some() {
                State::Ready
            } else {
                State::Broken
            };
            slots.push(Slot {
                id: LEGACY.into(),
                dir: root.to_owned(),
                mode,
                state,
            });
        }
        Err(_) => slots.push(Slot {
            id: LEGACY.into(),
            dir: root.to_owned(),
            mode: None,
            state: State::Broken,
        }),
    }
    let entries = match fs::read_dir(root.join("profiles")) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(slots),
        entries => entries.map_err(|e| e.to_string())?,
    };
    let mut profiles = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let Some(id) = entry.file_name().to_str().filter(|n| is_uuid(n)).map(str::to_owned) else {
            continue;
        };
        let slot = if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            read_slot(id, entry.path())
        } else {
            Slot {
                id,
                dir: entry.path(),
                mode: None,
                state: State::Broken,
            }
        };
        profiles.push(slot);
    }
    profiles.sort_by(|a, b| a.id.cmp(&b.id));
    slots.extend(profiles);
    Ok(slots)
}

/// Reserves a fresh slot for a key in `mode`, written as `pending` before any key exists.
pub fn reserve(root: &Path, mode: &'static str) -> Result<Slot, String> {
    let profiles = root.join("profiles");
    fs::create_dir_all(&profiles).map_err(|e| e.to_string())?;
    for _ in 0..8 {
        let id = new_uuid();
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
pub fn set_state(slot: &mut Slot, state: State) -> Result<(), String> {
    if slot.id == LEGACY || slot.state == State::Broken {
        return Err("Slot state cannot change".into());
    }
    write_atomic(&slot.dir.join("state"), state_name(state))?;
    slot.state = state;
    Ok(())
}

/// The slot named by `active-profile`, if the pointer is valid.
pub fn active(root: &Path) -> Option<String> {
    let value = read_marker(&root.join("active-profile")).ok()??;
    let id = value.strip_suffix('\n')?;
    (id == LEGACY || is_uuid(id)).then(|| id.to_owned())
}

pub fn set_active(root: &Path, id: &str) -> Result<(), String> {
    write_atomic(&root.join("active-profile"), &format!("{id}\n"))
}

/// The slot to open without asking: the active one if listed, else the only ready slot.
pub fn choose<'a>(root: &Path, slots: &'a [Slot]) -> Option<&'a Slot> {
    if let Some(id) = active(root) {
        if let Some(slot) = slots.iter().find(|slot| slot.id == id) {
            return Some(slot);
        }
    }
    match slots {
        [slot] if slot.state == State::Ready => Some(slot),
        _ => None,
    }
}

/// Reads a slot's key: `None` when its backend confirms there is no key.
pub type Load<'a> = &'a dyn Fn(&Slot) -> Result<Option<Identity>, String>;

pub enum Opened {
    /// No slot yet: offer the storage choice.
    Setup,
    Ready(Slot, Identity),
    /// Several slots and no valid pointer.
    Picker,
    /// The slot to open cannot be used as it is; the Peer ID is known when its key opens.
    Recovery(Slot, &'static str, Option<String>),
}

/// Opens the slot `choose` picks. A missing key or incomplete slot is a recovery state, never a
/// reason to create a new identity.
pub fn open(root: &Path, load: Load<'_>) -> Result<Opened, String> {
    let mut slots = list(root)?;
    let index = match choose(root, &slots) {
        Some(slot) => slots.iter().position(|s| s.id == slot.id),
        None if slots.len() == 1 => Some(0),
        None if slots.is_empty() => return Ok(Opened::Setup),
        None => None,
    };
    let Some(index) = index else {
        return Ok(Opened::Picker);
    };
    open_slot(slots.swap_remove(index), load)
}

/// Opens one listed slot; a provider error is returned, not turned into a recovery reason.
pub fn open_slot(slot: Slot, load: Load<'_>) -> Result<Opened, String> {
    let reason = match slot.state {
        State::Ready => match load(&slot)? {
            Some(identity) => return Ok(Opened::Ready(slot, identity)),
            None => "missing",
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

/// Creates an identity in a fresh slot and makes it active. The slot is marked ready only after
/// its key reopens with the same Peer ID.
pub fn create(
    root: &Path,
    mode: &'static str,
    save: &dyn Fn(&Slot, &Identity) -> Result<(), String>,
    load: Load<'_>,
) -> Result<(Slot, Identity), String> {
    let _lock = lock(root)?;
    let mut slot = reserve(root, mode)?;
    let identity = Identity::generate();
    save(&slot, &identity)?;
    match load(&slot)? {
        Some(saved) if saved.peer_id() == identity.peer_id() => {}
        _ => return Err("Saved identity does not reopen".into()),
    }
    set_state(&mut slot, State::Ready)?;
    set_active(root, &slot.id)?;
    Ok((slot, identity))
}

/// Removes slot `id`. A key that still exists must yield `peer_id`, the Peer ID the user
/// confirmed. The slot is marked `deleting` before `erase` and its files go only once `load`
/// confirms the key is absent, so any failure keeps a slot that a retry can finish. The legacy
/// slot and damaged slots are refused: they need a migration or hold files we cannot interpret.
pub fn remove(
    root: &Path,
    id: &str,
    peer_id: Option<&str>,
    load: Load<'_>,
    erase: &dyn Fn(&Slot) -> Result<(), String>,
) -> Result<(), String> {
    let _lock = lock(root)?;
    let mut slot = list(root)?
        .into_iter()
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
    // The rename drops the slot from `list` at once; a crash after it leaves only hidden markers.
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
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("dyapp-slots-test-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).unwrap();
        root
    }

    #[test]
    fn uuids_are_fresh_and_well_formed() {
        let (a, b) = (new_uuid(), new_uuid());
        assert!(is_uuid(&a) && is_uuid(&b) && a != b);
        assert!(!is_uuid("legacy-primary") && !is_uuid(&a.to_uppercase()));
    }

    #[test]
    fn legacy_install_is_the_only_slot_and_opens_without_a_pointer() {
        let root = temp_root();
        assert!(list(&root).unwrap().is_empty());
        fs::write(root.join("identity-storage"), "file\n").unwrap();
        let slots = list(&root).unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(
            (slots[0].id.as_str(), slots[0].mode),
            (LEGACY, Some("file"))
        );
        assert_eq!(slots[0].key_path(), root.join("identity.key"));
        assert_eq!(slots[0].system_name(), "primary");
        assert_eq!(choose(&root, &slots).unwrap().id, LEGACY);
        // A root key without its marker is a recovery state, not a fresh install.
        fs::remove_file(root.join("identity-storage")).unwrap();
        fs::write(root.join("identity.key"), [0; 32]).unwrap();
        let slots = list(&root).unwrap();
        assert_eq!((slots[0].mode, slots[0].state), (None, State::Broken));
        assert!(choose(&root, &slots).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reserved_slots_stay_pending_until_ready_and_need_a_pointer_when_several() {
        let root = temp_root();
        let mut first = reserve(&root, "system").unwrap();
        // A crash after reservation leaves a visible, incomplete slot that is not opened.
        let slots = list(&root).unwrap();
        assert_eq!(
            (slots[0].id.as_str(), slots[0].state),
            (first.id.as_str(), State::Pending)
        );
        assert!(choose(&root, &slots).is_none());
        set_state(&mut first, State::Ready).unwrap();
        assert_eq!(first.system_name(), format!("profile:{}", first.id));
        assert_eq!(choose(&root, &list(&root).unwrap()).unwrap().id, first.id);

        let second = reserve(&root, "file").unwrap();
        assert!(fs::read_dir(&second.dir)
            .unwrap()
            .all(|e| e.unwrap().file_name() != "identity.key"));
        assert!(choose(&root, &list(&root).unwrap()).is_none());
        set_active(&root, &second.id).unwrap();
        assert_eq!(choose(&root, &list(&root).unwrap()).unwrap().id, second.id);
        // A corrupt or dangling pointer falls back to the picker.
        fs::write(root.join("active-profile"), "../escape\n").unwrap();
        assert_eq!(active(&root), None);
        set_active(&root, &new_uuid()).unwrap();
        assert!(choose(&root, &list(&root).unwrap()).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn open_and_create_never_replace_a_key() {
        use std::cell::RefCell;
        use std::collections::HashMap;
        let root = temp_root();
        let keys = RefCell::new(HashMap::new());
        let save = |slot: &Slot, identity: &Identity| {
            keys.borrow_mut()
                .insert(slot.system_name(), *identity.secret());
            Ok::<_, String>(())
        };
        let load = |slot: &Slot| {
            Ok::<_, String>(
                keys.borrow()
                    .get(&slot.system_name())
                    .map(Identity::from_secret),
            )
        };
        let state = |root: &Path| match open(root, &load).unwrap() {
            Opened::Setup => "setup".to_owned(),
            Opened::Ready(slot, identity) => format!("ready {} {}", slot.id, identity.peer_id()),
            Opened::Picker => "picker".to_owned(),
            Opened::Recovery(slot, reason, peer) => format!("{reason} {} {peer:?}", slot.id),
        };
        assert_eq!(state(&root), "setup");
        let (first, alice) = create(&root, "system", &save, &load).unwrap();
        assert_eq!(
            state(&root),
            format!("ready {} {}", first.id, alice.peer_id())
        );
        let (second, bob) = create(&root, "file", &save, &load).unwrap();
        assert_eq!(
            state(&root),
            format!("ready {} {}", second.id, bob.peer_id())
        );
        fs::remove_file(root.join("active-profile")).unwrap();
        assert_eq!(state(&root), "picker");
        set_active(&root, &first.id).unwrap();
        keys.borrow_mut().remove(&first.system_name());
        assert_eq!(state(&root), format!("missing {} None", first.id));
        // A save that does not reopen leaves a pending slot and keeps the prior pointer.
        let lost = |_: &Slot, _: &Identity| Ok::<_, String>(());
        assert!(create(&root, "file", &lost, &load).is_err());
        assert_eq!(active(&root), Some(first.id.clone()));
        assert_eq!(list(&root).unwrap().len(), 3);
        // The sole slot opens for recovery even when incomplete, with its Peer ID if saved.
        let only = temp_root();
        let mut pending = reserve(&only, "system").unwrap();
        assert_eq!(state(&only), format!("incomplete {} None", pending.id));
        save(&pending, &alice).unwrap();
        let peer = Some(alice.peer_id());
        assert_eq!(state(&only), format!("incomplete {} {peer:?}", pending.id));
        set_state(&mut pending, State::Deleting).unwrap();
        assert_eq!(state(&only), format!("deleting {} {peer:?}", pending.id));
        fs::remove_dir_all(only).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remove_erases_only_the_confirmed_key() {
        use std::cell::{Cell, RefCell};
        use std::collections::HashMap;
        let root = temp_root();
        let keys = RefCell::new(HashMap::new());
        let save = |slot: &Slot, identity: &Identity| {
            keys.borrow_mut()
                .insert(slot.system_name(), *identity.secret());
            Ok::<_, String>(())
        };
        let load = |slot: &Slot| {
            Ok::<_, String>(
                keys.borrow()
                    .get(&slot.system_name())
                    .map(Identity::from_secret),
            )
        };
        let working = Cell::new(false);
        let erase = |slot: &Slot| {
            if working.get() {
                keys.borrow_mut().remove(&slot.system_name());
            }
            Ok::<_, String>(())
        };
        let state = |id: &str| {
            list(&root)
                .unwrap()
                .into_iter()
                .find(|s| s.id == id)
                .map(|s| s.state)
        };
        let (first, alice) = create(&root, "system", &save, &load).unwrap();
        let (second, bob) = create(&root, "file", &save, &load).unwrap();
        let (alice, bob) = (alice.peer_id(), bob.peer_id());
        // Another identity's Peer ID or none at all leaves the slot untouched.
        assert!(remove(&root, &second.id, Some(&alice), &load, &erase).is_err());
        assert!(remove(&root, &second.id, None, &load, &erase).is_err());
        assert_eq!(state(&second.id), Some(State::Ready));
        // An erase that leaves the key keeps the slot, now deleting, for a retry.
        assert!(remove(&root, &second.id, Some(&bob), &load, &erase).is_err());
        assert_eq!(state(&second.id), Some(State::Deleting));
        working.set(true);
        // Network and node settings and the node's stores go with the slot.
        fs::write(second.dir.join("network-draft"), "dyapp-network 1\nname \n").unwrap();
        fs::write(second.dir.join("node.json"), "{}").unwrap();
        fs::create_dir_all(second.dir.join("node/profiles")).unwrap();
        fs::write(second.dir.join("client.db"), "draft").unwrap();
        fs::write(second.dir.join("client.db-wal"), "draft").unwrap();
        remove(&root, &second.id, Some(&bob), &load, &erase).unwrap();
        assert_eq!(state(&second.id), None);
        assert_eq!(active(&root), None);
        assert_eq!(fs::read_dir(root.join("profiles")).unwrap().count(), 1);
        assert!(keys.borrow().contains_key(&first.system_name()));
        // A key already gone needs no Peer ID; foreign files keep the slot.
        let notes = reserve(&root, "file").unwrap();
        fs::write(notes.dir.join("notes.txt"), "mine").unwrap();
        assert!(remove(&root, &notes.id, None, &load, &erase).is_err());
        assert_eq!(state(&notes.id), Some(State::Deleting));
        keys.borrow_mut().remove(&first.system_name());
        remove(&root, &first.id, None, &load, &erase).unwrap();
        assert_eq!(state(&first.id), None);
        fs::write(root.join("identity-storage"), "file\n").unwrap();
        assert!(remove(&root, LEGACY, None, &load, &erase).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tampered_slots_are_broken_and_foreign_names_skipped() {
        let root = temp_root();
        let mut slot = reserve(&root, "file").unwrap();
        fs::create_dir(root.join("profiles/notes")).unwrap();
        fs::write(slot.dir.join("state"), "exploded\n").unwrap();
        let slots = list(&root).unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].state, State::Broken);
        slot.state = State::Broken;
        assert!(set_state(&mut slot, State::Ready).is_err());
        #[cfg(unix)]
        {
            let id = new_uuid();
            std::os::unix::fs::symlink(&slot.dir, root.join("profiles").join(&id)).unwrap();
            fs::remove_file(slot.dir.join("state")).unwrap();
            std::os::unix::fs::symlink("/dev/null", slot.dir.join("state")).unwrap();
            let slots = list(&root).unwrap();
            assert!(slots.iter().all(|slot| slot.state == State::Broken));
            assert!(slots.iter().any(|slot| slot.id == id));
        }
        fs::remove_dir_all(root).unwrap();
    }
}
