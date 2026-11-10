//! Exact identity-owned paths; installation settings and node data are never admitted.

use std::fs;
use std::io;
use std::path::Path;

const FILES: &[&str] = &[
    "identity.key",
    "identity.key.tmp",
    "device.key",
    "device.key.tmp",
    "seal.key",
    "seal.key.tmp",
    "chat-state.key",
    "chat-state.key.tmp",
    "state",
    "state.tmp",
    "storage-mode",
    "storage-mode.tmp",
    "network",
    "network.tmp",
    "network-draft",
    "network-draft.tmp",
    "client.db",
    "client.db-wal",
    "client.db-shm",
    "client.db-journal",
    "profile-draft.json",
    "profile-draft.json.bak",
    "profile-draft.json.new",
];

fn invalid() -> io::Error {
    io::Error::other("The identity folder contains an unexpected path")
}

fn hex(name: &str, length: usize) -> bool {
    name.len() == length
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn media(path: &Path, stickers: bool) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(invalid)?;
        let pending = stickers
            && name
                .strip_prefix('.')
                .and_then(|value| value.strip_suffix(".pending"))
                .is_some_and(|value| hex(value, 32));
        if !(hex(name, 64) || pending) || !entry.file_type()?.is_file() {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn validate(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(invalid()),
        Err(error) => return Err(error),
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(invalid)?;
        let kind = entry.file_type()?;
        if FILES.contains(&name) && kind.is_file() {
            continue;
        }
        if matches!(name, "originals" | "sticker-packs") && kind.is_dir() {
            media(&entry.path(), name == "sticker-packs")?;
        } else {
            return Err(invalid());
        }
    }
    Ok(true)
}

/// Retain the empty slot directory so a failure is still discoverable as deleting.
pub(super) fn clear(path: &Path) -> io::Result<()> {
    if !validate(path)? {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            // Validation admits only flat media directories; std protects symlink races.
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
