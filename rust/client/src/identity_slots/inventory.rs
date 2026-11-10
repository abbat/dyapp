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

fn candidate(path: &Path, id: String, layout: Layout) -> io::Result<Slot> {
    let directory = fs::symlink_metadata(path)?.is_dir();
    let storage = if !directory {
        None
    } else if layout == Layout::Desktop {
        markers::storage(&path.join("storage-mode"))
    } else {
        Some(Storage::Wrapped)
    };
    let state = if directory && storage.is_some() {
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
        if is_uuid(id) {
            candidates.insert(id.to_owned(), candidate(&entry.path(), id.into(), layout)?);
        } else if layout == Layout::Ios {
            if let Some(id) = id.strip_suffix(".deleting").filter(|id| is_uuid(id)) {
                if markers::state(&entry.path()) == State::Deleting {
                    deleting.push(id.to_owned());
                }
            }
        }
    }
    for id in deleting {
        let slot = candidates.entry(id.clone()).or_insert_with(|| Slot {
            dir: profiles.join(&id),
            id,
            state: State::Deleting,
            storage: Some(Storage::Wrapped),
        });
        slot.state = State::Deleting;
    }
    slots.extend(candidates.into_values());
    Ok(slots)
}
