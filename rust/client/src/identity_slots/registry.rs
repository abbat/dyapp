use super::{KeyReader, Layout, Resolution, Slot};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

/// Shared registry owner; cooperating operations use the same process-wide file lock.
pub struct Registry {
    pub(super) root: PathBuf,
    pub(super) layout: Layout,
}

impl Registry {
    /// Bind existing storage without creating directories, keys or markers.
    #[must_use]
    pub fn new(root: PathBuf, layout: Layout) -> Self {
        Self { root, layout }
    }

    /// List candidates while excluding cooperating mutations.
    ///
    /// # Errors
    /// Returns lock or filesystem failures, never an empty replacement inventory.
    pub fn list(&self) -> Result<Vec<Slot>, String> {
        let _guard = self.lock()?;
        super::list(&self.root, self.layout).map_err(|error| error.to_string())
    }

    /// Resolve an existing key while excluding cooperating mutations.
    ///
    /// # Errors
    /// Returns lock or inventory failures; platform key failures remain recovery states.
    pub fn resolve(&self, keys: &dyn KeyReader) -> Result<Resolution, String> {
        let _guard = self.lock()?;
        super::resolve(&self.root, self.layout, keys)
    }

    pub(super) fn lock(&self) -> Result<File, String> {
        fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let name = if self.layout == Layout::Ios {
            ".identity-slots.lock"
        } else {
            "profiles.lock"
        };
        lock_file(&self.root.join(name))
    }
}

fn lock_file(path: &Path) -> Result<File, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err("Registry lock is not a regular file".into())
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.to_string()),
        _ => {}
    }
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(|error| error.to_string())?;
    file.lock().map_err(|error| error.to_string())?;
    Ok(file)
}
