//! In-process frontend view generations; durable source admission remains separate.

use std::sync::{Arc, Mutex, MutexGuard};

/// One cooperating frontend's transition/admission scope, not a cross-process registry epoch.
#[derive(Debug, Default)]
pub struct ProfileView(Mutex<Arc<()>>);

/// Opaque generation whose retained allocation prevents ABA reuse.
#[derive(Clone, Debug)]
pub struct ProfileGeneration(Arc<()>);

/// Keep this guard through source capture or the admitted mutation, never network work.
#[must_use = "dropping the guard releases profile-view admission"]
pub struct ProfileViewGuard<'a>(MutexGuard<'a, Arc<()>>);

impl ProfileView {
    /// Serialize source capture with cooperating transitions.
    ///
    /// # Errors
    /// Refuses a poisoned view instead of fabricating a valid generation.
    pub fn current(&self) -> Result<ProfileViewGuard<'_>, String> {
        self.0
            .lock()
            .map(ProfileViewGuard)
            .map_err(|_| "Profile view lock poisoned".into())
    }

    /// Invalidate older work before a transition, even when its mutation later fails.
    ///
    /// # Errors
    /// Refuses a poisoned view.
    pub fn transition(&self) -> Result<ProfileViewGuard<'_>, String> {
        let mut guard = self.current()?;
        *guard.0 = Arc::new(());
        Ok(guard)
    }

    /// Admit work only in its captured generation, serialized with transitions.
    ///
    /// # Errors
    /// Refuses stale/foreign generations and poisoned state.
    pub fn admit(&self, generation: &ProfileGeneration) -> Result<ProfileViewGuard<'_>, String> {
        let guard = self.current()?;
        if !Arc::ptr_eq(&guard.0, &generation.0) {
            return Err("Profile view changed".into());
        }
        Ok(guard)
    }
}

impl ProfileViewGuard<'_> {
    /// Capture a generation without retaining this guard or any identity-operation lease.
    #[must_use]
    pub fn generation(&self) -> ProfileGeneration {
        ProfileGeneration(Arc::clone(&self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_invalidates_prior_work_even_when_the_same_owner_returns() {
        let view = ProfileView::default();
        let first = view.current().unwrap().generation();
        let away = view.transition().unwrap().generation();
        let back = view.transition().unwrap().generation();
        assert!(view.admit(&first).is_err());
        assert!(view.admit(&away).is_err());
        let guard = view.admit(&back).unwrap();
        assert!(matches!(
            view.0.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        drop(guard);
        assert!(view.0.try_lock().is_ok());
    }

    #[test]
    fn a_poisoned_view_cannot_mint_or_admit_generations() {
        let view = ProfileView::default();
        let old = view.current().unwrap().generation();
        let unwound = std::panic::catch_unwind(|| {
            let _guard = view.transition().unwrap();
            panic!("controlled transition unwind");
        });
        assert!(unwound.is_err());
        assert!(view.current().is_err());
        assert!(view.transition().is_err());
        assert!(view.admit(&old).is_err());
    }

    #[test]
    fn another_view_cannot_admit_a_token_and_failed_transitions_still_invalidate() {
        let view = ProfileView::default();
        let old = view.current().unwrap().generation();
        let foreign = ProfileView::default().current().unwrap().generation();
        assert!(view.admit(&foreign).is_err());
        let failed: Result<(), &str> = {
            let _guard = view.transition().unwrap();
            Err("mutation failed")
        };
        assert!(failed.is_err());
        assert!(view.admit(&old).is_err());
        let current = view.current().unwrap().generation();
        assert!(view.admit(&current).is_ok());
    }
}
