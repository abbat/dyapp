//! Local identity slots: `profiles/<uuid>/` directories beside the legacy root key, discovered by
//! a directory scan rather than a registry; see docs/development/desktop-identity-storage.md.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use dyapp_identity::Identity;

/// The virtual slot of the root `identity.key` / `identity-storage` installation.
pub const LEGACY: &str = "legacy-primary";

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
