//! Desktop display cache bound to the core authority for one canonical identity directory.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::contact_verification::{self, Context, Generation};
use dyapp_client::profiles::Profiles;
use dyapp_profile::VerifiedProfile;

use crate::peer_photos::PhotoCache;

/// Verified profiles seen in the active identity and network context.
#[derive(Default)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct SearchState(pub Arc<Mutex<ProfileCache>>);

/// Profile authority and desktop photos share one activation boundary.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct ProfileCache {
    epoch: u64,
    observed: Generation,
    pub(crate) context: Arc<Mutex<Context>>,
    pub photos: PhotoCache,
}

impl Default for ProfileCache {
    fn default() -> Self {
        let context = Context::default();
        let observed = context.generation();
        Self {
            epoch: 0,
            observed,
            context: Arc::new(Mutex::new(context)),
            photos: PhotoCache::default(),
        }
    }
}

impl ProfileCache {
    /// Bind before reading settings or starting identity-bound work.
    pub(crate) fn bind(&mut self, dir: &Path) -> Result<u64, String> {
        let shared = contact_verification::shared(dir).map_err(|e| e.to_string())?;
        let observed = {
            let context = shared.lock().map_err(|e| e.to_string())?;
            if Arc::ptr_eq(&self.context, &shared) && context.is_current(&self.observed) {
                return Ok(self.epoch);
            }
            context.generation()
        };
        self.epoch = self.epoch.checked_add(1).ok_or("context epoch exhausted")?;
        self.context = shared;
        self.observed = observed;
        self.photos = PhotoCache::default();
        Ok(self.epoch)
    }

    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn snapshot(&self) -> Result<(u64, Profiles), String> {
        let context = self.context.lock().map_err(|e| e.to_string())?;
        if !context.is_current(&self.observed) {
            return Err("notConnected".into());
        }
        Ok((self.epoch, context.profiles().clone()))
    }

    #[cfg(test)]
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn current(&self, epoch: u64) -> Result<bool, String> {
        let context = self.context.lock().map_err(|e| e.to_string())?;
        Ok(self.epoch == epoch && context.is_current(&self.observed))
    }

    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn accept_if_current(
        &mut self,
        epoch: u64,
        profile: VerifiedProfile,
    ) -> Result<bool, String> {
        Ok(self
            .with_current(epoch, |context, _| context.accept(profile).is_ok())?
            .unwrap_or(false))
    }

    /// Finalize a result while shared activation is excluded.
    pub(crate) fn with_current<T>(
        &mut self,
        epoch: u64,
        apply: impl FnOnce(&mut Context, &mut PhotoCache) -> T,
    ) -> Result<Option<T>, String> {
        let mut context = self.context.lock().map_err(|e| e.to_string())?;
        if self.epoch != epoch || !context.is_current(&self.observed) {
            return Ok(None);
        }
        Ok(Some(apply(&mut context, &mut self.photos)))
    }

    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn clear(&mut self) -> Result<(), String> {
        let mut context = self.context.lock().map_err(|e| e.to_string())?;
        context.clear();
        self.photos = PhotoCache::default();
        // Exhaustion must leave the observed generation stale, never revive an old epoch.
        self.epoch = self.epoch.checked_add(1).ok_or("context epoch exhausted")?;
        self.observed = context.generation();
        Ok(())
    }
}

#[cfg(test)]
#[path = "cache/tests.rs"]
mod tests;

impl SearchState {
    /// Discard verified profiles held for the previous active context.
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn clear(&self) -> Result<(), String> {
        self.0.lock().map_err(|e| e.to_string())?.clear()
    }

    /// Invalidate old work before any checked network activation attempt.
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn activate_network(
        &self,
        checks: &crate::network::NetworkState,
        dir: &Path,
    ) -> Result<crate::network::Draft, String> {
        self.activate_with(dir, || crate::network::activate(checks, dir))
    }

    pub(super) fn activate_with<T>(
        &self,
        dir: &Path,
        activate: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let mut cache = self.0.lock().map_err(|e| e.to_string())?;
        cache.bind(dir)?;
        cache.clear()?;
        // clear() releases the core guard before Checks acquires it.
        let result = activate();
        cache.bind(dir)?;
        result
    }
}
