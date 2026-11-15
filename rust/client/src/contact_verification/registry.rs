//! In-process contact authority shared by workers for one canonical identity directory.
use super::Context;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

type Entries = HashMap<PathBuf, Weak<Mutex<Context>>>;
static CONTEXTS: OnceLock<Mutex<Entries>> = OnceLock::new();

/// Share contact authority with every worker and active-network writer for this directory.
/// Retain the returned handle through actual worker completion, including UI cancellation.
/// Lock it when capturing a ticket/network and again when accepting its result. Activation
/// clears the same context while holding its lock through the active-settings write.
///
/// # Errors
/// Rejects a missing, non-directory or symlink identity root; canonicalization and poisoned
/// registry failures propagate without creating or changing any file.
pub fn shared(dir: &Path) -> io::Result<Arc<Mutex<Context>>> {
    if !dir.is_dir() || dir.is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid identity directory",
        ));
    }
    let directory = dir.canonicalize()?;
    let mut contexts = CONTEXTS
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| io::Error::other("contact context registry is poisoned"))?;
    contexts.retain(|_, context| context.strong_count() != 0);
    if let Some(context) = contexts.get(&directory).and_then(Weak::upgrade) {
        return Ok(context);
    }
    let context = Arc::new(Mutex::new(Context::default()));
    contexts.insert(directory, Arc::downgrade(&context));
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::temp_db_path;

    #[test]
    fn canonical_aliases_share_authority_without_retaining_idle_workers() {
        let path = temp_db_path();
        std::fs::create_dir_all(path.join("child")).unwrap();
        let first = shared(&path).unwrap();
        let alias = shared(&path.join("child").join("..")).unwrap();
        assert!(Arc::ptr_eq(&first, &alias));
        let held = first.lock().unwrap();
        assert!(matches!(
            alias.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        drop(held);
        assert!(alias.try_lock().is_ok());
        let other = shared(&path.join("child")).unwrap();
        assert!(!Arc::ptr_eq(&first, &other));
        let weak = Arc::downgrade(&first);
        drop(first);
        drop(alias);
        assert!(weak.upgrade().is_none());
        assert!(shared(&path).is_ok());
    }

    #[test]
    fn missing_roots_are_never_created_by_lookup() {
        let path = temp_db_path().join("absent-context");
        assert!(shared(&path).is_err());
        assert!(!path.exists());
    }
}
