//! Generation observation has a separate memory-only publication lock, never the contact IO lock.
use super::Generation;
use std::sync::{Arc, RwLock};

/// Observe current context tokens without retaining the Context or locking its IO-bearing guard.
/// The publication guard never escapes these constant-time clone/replace operations.
#[derive(Clone, Debug, Default)]
pub struct GenerationObserver(Arc<RwLock<Arc<()>>>);

impl GenerationObserver {
    /// Snapshot a unique opaque token. A poisoned publication lock refuses capture.
    #[must_use]
    pub fn capture(&self) -> Option<Generation> {
        self.0
            .read()
            .ok()
            .map(|current| Generation(Arc::clone(&current)))
    }

    pub(super) fn advance(&self) {
        let next = Arc::new(());
        if let Ok(mut current) = self.0.write() {
            *current = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contact_verification::Context;

    #[test]
    fn observation_tracks_clear_without_restoring_old_tokens() {
        let mut context = Context::default();
        let observer = context.generation_observer();
        let old = observer.capture().unwrap();
        assert!(context.is_current(&old));
        assert!(!Context::default().is_current(&old));
        context.clear();
        let current = observer.capture().unwrap();
        assert!(!context.is_current(&old));
        assert!(context.is_current(&current));
        context.clear();
        assert!(!context.is_current(&old));
        assert!(!context.is_current(&current));
        assert!(context.is_current(&observer.capture().unwrap()));
    }

    #[test]
    fn poisoned_observation_never_grants_current_authority() {
        let mut context = Context::default();
        let observer = context.generation_observer();
        let old = observer.capture().unwrap();
        let publication = Arc::clone(&observer.0);
        let broken = std::thread::spawn(move || {
            let _guard = publication.write().unwrap();
            panic!("controlled publication poison");
        });
        assert!(broken.join().is_err());
        assert!(observer.capture().is_none());
        assert!(!context.is_current(&old));
        assert!(!context.is_current(&context.generation()));
        context.clear();
        assert!(observer.capture().is_none());
        assert!(!context.is_current(&old));
    }
}
