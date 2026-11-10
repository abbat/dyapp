use super::{is_uuid, markers, Layout, Slot, State, Storage, LEGACY};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

fn legacy(root: &Path, layout: Layout) -> io::Result<Option<Slot>> {
    let key = match fs::symlink_metadata(root.join("identity.key")) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        result => Some(result?),
    };
    let (present, storage) = if layout == Layout::Desktop {
        let marker = markers::read(&root.join("identity-storage"));
        let present = !matches!(&marker, Ok(None)) || key.is_some();
        (present, markers::storage(&root.join("identity-storage")))
    } else {
        (
            key.is_some(),
            key.filter(fs::Metadata::is_file).map(|_| Storage::Wrapped),
        )
    };
    Ok(present.then(|| Slot {
        id: LEGACY.into(),
        dir: root.into(),
        state: if storage.is_some() {
            State::Ready
        } else {
            State::Broken
        },
        storage,
    }))
}

fn candidate(path: &Path, id: String, layout: Layout, removed: bool) -> io::Result<Slot> {
    let directory = fs::symlink_metadata(path)?.is_dir();
    let storage = if !directory {
        None
    } else if layout == Layout::Desktop {
        markers::storage(&path.join("storage-mode"))
    } else {
        Some(Storage::Wrapped)
    };
    let state = if directory && removed {
        State::Deleting
    } else if directory && storage.is_some() {
        markers::state(&path.join("state"))
    } else {
        State::Broken
    };
    Ok(Slot {
        id,
        dir: path.into(),
        state,
        storage,
    })
}

fn insert(candidates: &mut BTreeMap<String, Slot>, slot: Slot) {
    use std::collections::btree_map::Entry;
    match candidates.entry(slot.id.clone()) {
        Entry::Vacant(entry) => {
            entry.insert(slot);
        }
        Entry::Occupied(mut entry) => {
            let existing = entry.get_mut();
            existing.dir = existing.dir.with_file_name(&existing.id);
            existing.state = State::Broken;
            existing.storage = None;
        }
    }
}

fn named_candidate(path: &Path, name: &str, layout: Layout) -> io::Result<Option<Slot>> {
    if is_uuid(name) {
        return candidate(path, name.into(), layout, false).map(Some);
    }
    let hidden = name
        .strip_prefix('.')
        .and_then(|id| id.strip_suffix(".removed"))
        .filter(|id| is_uuid(id) && layout != Layout::Ios);
    hidden
        .map(|id| candidate(path, id.into(), layout, true))
        .transpose()
}

fn deleting_id<'a>(path: &Path, name: &'a str, layout: Layout) -> Option<&'a str> {
    name.strip_suffix(".deleting").filter(|id| {
        layout == Layout::Ios && is_uuid(id) && markers::state(path) == State::Deleting
    })
}

fn apply_tombstone(candidates: &mut BTreeMap<String, Slot>, profiles: &Path, id: String) {
    let slot = candidates.entry(id.clone()).or_insert_with(|| Slot {
        dir: profiles.join(&id),
        id,
        state: State::Deleting,
        storage: Some(Storage::Wrapped),
    });
    let cleaned_marker = slot.storage == Some(Storage::Wrapped)
        && matches!(markers::read(&slot.dir.join("state")), Ok(None));
    if slot.state != State::Broken || cleaned_marker {
        slot.state = State::Deleting;
    }
}

/// Enumerate existing candidates without opening keys or changing any files.
///
/// # Errors
/// Returns filesystem failures reading the inventory; malformed per-slot metadata is Broken.
pub fn list(root: &Path, layout: Layout) -> io::Result<Vec<Slot>> {
    let mut slots: Vec<_> = legacy(root, layout)?.into_iter().collect();
    let profiles = root.join("profiles");
    match fs::symlink_metadata(&profiles) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(slots),
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(io::Error::other("Profiles is not a directory")),
        Err(error) => return Err(error),
    }
    let mut candidates = BTreeMap::new();
    let mut deleting = Vec::new();
    for entry in fs::read_dir(&profiles)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(id) = name.to_str() else { continue };
        if let Some(slot) = named_candidate(&entry.path(), id, layout)? {
            insert(&mut candidates, slot);
        } else if let Some(id) = deleting_id(&entry.path(), id, layout) {
            deleting.push(id.to_owned());
        }
    }
    for id in deleting {
        apply_tombstone(&mut candidates, &profiles, id);
    }
    slots.extend(candidates.into_values());
    Ok(slots)
}
