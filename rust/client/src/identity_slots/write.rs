use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub(super) fn fresh(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

/// Persistence status after an active selection is already visibly committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionDurability {
    /// File and parent-directory synchronization completed.
    Synced,
    /// Rename completed, but directory persistence is not confirmed.
    Unconfirmed {
        /// Diagnostic for the failed or unavailable synchronization.
        detail: String,
    },
}

pub(super) fn select(path: &Path, bytes: &[u8]) -> Result<SelectionDurability, String> {
    select_with(path, bytes, selection_sync)
}

pub(super) fn select_with(
    path: &Path,
    bytes: &[u8],
    sync: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<SelectionDurability, String> {
    replace_then(path, bytes, |parent| {
        Ok(match sync(parent) {
            Ok(()) => SelectionDurability::Synced,
            Err(detail) => SelectionDurability::Unconfirmed { detail },
        })
    })
}

pub(super) fn selection_sync(parent: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        sync_directory(parent)
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        Err("Directory persistence is not confirmed on this platform".into())
    }
}

pub(super) fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    replace_then(path, bytes, sync_directory)
}

fn replace_then<T>(
    path: &Path,
    bytes: &[u8],
    after: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    let temp = std::path::PathBuf::from(name);
    // A stranded temporary file fails closed; it is never followed, truncated or deleted.
    fresh(&temp, bytes)?;
    let parent = path.parent().ok_or("Marker has no parent directory")?;
    fs::rename(&temp, path).map_err(|error| error.to_string())?;
    after(parent)
}

pub(super) fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
