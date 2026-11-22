//! In-process contact authority shared by workers for one canonical identity directory.
use super::Context;
use crate::scores::Scores;
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

/// Move every live context's open circuits to half-open after a platform resume.
///
/// # Errors
/// Returns poisoned registry/context failures without issuing network traffic.
pub async fn resume_all() -> io::Result<()> {
    let contexts: Vec<_> = {
        let mut entries = CONTEXTS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| io::Error::other("contact context registry is poisoned"))?;
        entries.retain(|_, context| context.strong_count() != 0);
        entries.values().filter_map(Weak::upgrade).collect()
    };
    for context in contexts {
        let scores = {
            let context = context
                .lock()
                .map_err(|_| io::Error::other("contact context is poisoned"))?;
            context.scores()
        };
        scores.lock().await.resume();
    }
    Ok(())
}

/// Share only the async selection state, releasing the context registry lock before network I/O.
///
/// # Errors
/// Returns the same directory and registry failures as [`shared`].
pub fn scores(dir: &Path) -> io::Result<Arc<tokio::sync::Mutex<Scores>>> {
    let context = shared(dir)?;
    let context = context
        .lock()
        .map_err(|_| io::Error::other("contact context registry is poisoned"))?;
    Ok(context.scores())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_aliases_share_authority_without_retaining_idle_workers() {
        let test_dir = dyapp_test_support::test_dir().unwrap();
        let path = test_dir.path().join("client-1.db");
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
    fn clear_replaces_selection_without_waiting_for_an_old_operation() {
        let test_dir = dyapp_test_support::test_dir().unwrap();
        let path = test_dir.path().join("client-1.db");
        std::fs::create_dir_all(&path).unwrap();
        let context = shared(&path).unwrap();
        let old = context.lock().unwrap().scores();
        context.lock().unwrap().clear();
        let new = context.lock().unwrap().scores();
        assert!(!Arc::ptr_eq(&old, &new));
        assert!(old.try_lock().is_ok());
    }

    #[tokio::test]
    async fn resume_moves_a_live_open_circuit_to_half_open() {
        let test_dir = dyapp_test_support::test_dir().unwrap();
        let path = test_dir.path().join("client-1.db");
        std::fs::create_dir_all(&path).unwrap();
        let context = shared(&path).unwrap();
        let scores = context.lock().unwrap().scores();
        let peer = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        {
            let mut scores = scores.lock().await;
            scores.register(peer, libp2p::Multiaddr::empty(), String::new());
            scores.invalid(peer, crate::selection::Operation::Search);
            assert_eq!(
                scores.state(peer, std::time::Instant::now()),
                crate::selection::State::Open
            );
        }
        resume_all().await.unwrap();
        assert_eq!(
            scores.lock().await.state(peer, std::time::Instant::now()),
            crate::selection::State::HalfOpen
        );
    }

    #[test]
    fn missing_roots_are_never_created_by_lookup() {
        let test_dir = dyapp_test_support::test_dir().unwrap();
        let path = test_dir.path().join("client-1.db").join("absent-context");
        assert!(shared(&path).is_err());
        assert!(!path.exists());
    }
}
