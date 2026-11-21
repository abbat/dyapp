//! Per-field privacy adapters; defaults, inversion and validation belong to the common Store.
use super::{failed, ChatSession};
use crate::CoreError;

#[uniffi::export]
impl ChatSession {
    /// Whether optional profile-view signals may be sent; the common default is off.
    ///
    /// # Errors
    /// Returns session-lock, database or malformed-setting errors.
    pub fn profile_views_enabled(&self) -> Result<bool, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session.store().send_profile_views().map_err(failed)
    }

    /// Persist only the profile-view preference, without sending a signal.
    ///
    /// # Errors
    /// Returns session-lock or persistence errors.
    pub fn set_profile_views_enabled(&self, enabled: bool) -> Result<(), CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session
            .store()
            .set_send_profile_views(enabled)
            .map_err(failed)
    }

    /// Whether typing signals may be sent; the common default is on.
    ///
    /// # Errors
    /// Returns session-lock, database or malformed-setting errors.
    pub fn typing_enabled(&self) -> Result<bool, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session.store().send_typing().map_err(failed)
    }

    /// Persist only the typing preference, without sending a signal.
    ///
    /// # Errors
    /// Returns session-lock or persistence errors.
    pub fn set_typing_enabled(&self, enabled: bool) -> Result<(), CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session.store().set_send_typing(enabled).map_err(failed)
    }
}
