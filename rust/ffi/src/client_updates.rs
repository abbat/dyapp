//! Thin installation-scoped update state/actions; no HTTP or activation is initiated here.
use crate::CoreError;
use dyapp_client::updates as core;
use std::path::PathBuf;
use std::sync::Arc;

mod types;
pub use types::{
    MobileUpdateAvailability, MobileUpdateDelivery, MobileUpdateInterval, MobileUpdateNoticeAction,
    MobileUpdateOffer, MobileUpdatePlatform, MobileUpdateState,
};

/// Bind once to the private application root, independently of the selected identity.
/// Call disk operations on an IO worker; this object owns no identity/node operation lease.
#[derive(uniffi::Object)]
pub struct MobileClientUpdates {
    root: PathBuf,
    platform: core::feed::Platform,
    installed: core::Version,
}

#[uniffi::export]
impl MobileClientUpdates {
    /// Bind metadata without reading files, creating keys or starting requests.
    ///
    /// # Errors
    /// Refuses relative roots or an invalid native build version.
    #[uniffi::constructor]
    pub fn new(root: String, platform: MobileUpdatePlatform) -> Result<Arc<Self>, CoreError> {
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err(CoreError::Failed("invalidUpdateRoot".into()));
        }
        let installed = core::Version::parse(env!("CARGO_PKG_VERSION"))
            .ok_or_else(|| CoreError::Failed("invalidClientVersion".into()))?;
        Ok(Arc::new(Self {
            root,
            platform: platform.into(),
            installed,
        }))
    }

    /// Read core-derived state, including during an update request.
    ///
    /// # Errors
    /// Returns storage/validation failures; never fabricates empty state on contention.
    pub fn read_state(&self) -> Result<MobileUpdateState, CoreError> {
        let snapshot = core::checker::read_state(&self.root)
            .map_err(|error| CoreError::Failed(error.to_string()))?;
        Ok(self.project(&snapshot))
    }

    /// Persist the selected interval immediately without starting/canceling HTTP.
    ///
    /// # Errors
    /// Returns storage/validation failures without replacing known trusted state.
    pub fn set_interval(
        &self,
        interval: MobileUpdateInterval,
    ) -> Result<MobileUpdateState, CoreError> {
        let snapshot = core::checker::set_interval(&self.root, interval.into())
            .map_err(|error| CoreError::Failed(error.to_string()))?;
        Ok(self.project(&snapshot))
    }
    /// Dismiss exactly the displayed version after committing shared installation state.
    ///
    /// # Errors
    /// Refuses malformed versions or persistence failures without acknowledging success.
    pub fn dismiss_notice(
        &self,
        expected_version: &str,
    ) -> Result<MobileUpdateNoticeAction, CoreError> {
        let expected = core::Version::parse(expected_version)
            .ok_or_else(|| CoreError::Failed("invalidUpdateVersion".into()))?;
        self.notice_result(core::notices::dismiss(
            &self.root,
            self.platform,
            self.installed,
            expected,
        ))
    }

    /// Claim one OS notification only after the native caller has confirmed permission.
    /// A true applied result is durable; this method never requests an OS notification.
    ///
    /// # Errors
    /// Refuses malformed versions or persistence failures; no OS request is permitted on error.
    pub fn claim_notification(
        &self,
        expected_version: &str,
    ) -> Result<MobileUpdateNoticeAction, CoreError> {
        let expected = core::Version::parse(expected_version)
            .ok_or_else(|| CoreError::Failed("invalidUpdateVersion".into()))?;
        self.notice_result(core::notices::claim_notification(
            &self.root,
            self.platform,
            self.installed,
            expected,
        ))
    }
}

impl MobileClientUpdates {
    fn notice_result(
        &self,
        result: Result<(core::snapshot::Snapshot, bool), core::storage::Error>,
    ) -> Result<MobileUpdateNoticeAction, CoreError> {
        let (snapshot, applied) = result.map_err(|error| CoreError::Failed(error.to_string()))?;
        Ok(MobileUpdateNoticeAction {
            state: self.project(&snapshot),
            applied,
        })
    }

    fn project(&self, snapshot: &core::snapshot::Snapshot) -> MobileUpdateState {
        use dyapp_client::updates::availability::Availability as A;
        let (availability, offer) =
            match core::availability::project(snapshot, self.platform, self.installed) {
                A::NeverChecked => (MobileUpdateAvailability::NeverChecked, None),
                A::NotPublished => (MobileUpdateAvailability::NotPublished, None),
                A::Current => (MobileUpdateAvailability::Current, None),
                A::DestinationUnavailable => {
                    (MobileUpdateAvailability::DestinationUnavailable, None)
                }
                A::Available(offer) => (MobileUpdateAvailability::Available, Some((&offer).into())),
            };
        let checkpoint = snapshot.checkpoint();
        MobileUpdateState {
            installed_version: self.installed.to_string(),
            interval: checkpoint.interval.into(),
            last_attempt: checkpoint.state.last_attempt,
            last_success: checkpoint.state.last_success,
            in_flight: checkpoint.in_flight,
            availability,
            offer,
            notice: snapshot
                .notice(self.platform, self.installed)
                .as_ref()
                .map(Into::into),
        }
    }
}

#[cfg(test)]
mod tests;
