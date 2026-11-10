use super::{is_uuid, Active, Layout, State, Storage, LEGACY};
use std::fs;
use std::io::{self, Read};
use std::path::Path;

pub(super) fn read(path: &Path) -> io::Result<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        metadata => metadata?,
    };
    if !metadata.is_file() {
        return Err(io::Error::other("Marker is not a regular file"));
    }
    let mut value = String::new();
    fs::File::open(path)?.take(65).read_to_string(&mut value)?;
    if value.len() > 64 {
        return Err(io::Error::other("Marker exceeds 64 bytes"));
    }
    Ok(Some(value))
}

pub(super) fn state(path: &Path) -> State {
    match read(path).ok().flatten().as_deref() {
        Some("pending\n") => State::Pending,
        Some("ready\n") => State::Ready,
        Some("deleting\n") => State::Deleting,
        _ => State::Broken,
    }
}

pub(super) fn storage(path: &Path) -> Option<Storage> {
    match read(path).ok().flatten().as_deref() {
        Some("system\n") => Some(Storage::System),
        Some("file\n") => Some(Storage::File),
        _ => None,
    }
}

/// Read the active pointer without rewriting invalid or absent metadata.
#[must_use]
pub fn active(root: &Path, layout: Layout) -> Active {
    let text = match read(&root.join("active-profile")) {
        Ok(None) => return Active::Absent,
        Ok(Some(value)) => value,
        Err(_) => return Active::Invalid,
    };
    let id = match text.strip_suffix('\n') {
        Some(id) => id,
        None if layout == Layout::Android => &text,
        None => return Active::Invalid,
    };
    match id {
        "none" if layout == Layout::Ios => Active::Unselected,
        LEGACY => Active::Selected(id.into()),
        _ if is_uuid(id) => Active::Selected(id.into()),
        _ => Active::Invalid,
    }
}
