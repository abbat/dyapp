//! Installed-target availability from a snapshot accepted by the shared checker/storage.
use super::feed::{Method, Platform};
use super::snapshot::Snapshot;
use super::Version;

/// A newer target with a fixed official information destination, never a feed-supplied URL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Offer {
    version: Version,
    method: Method,
    release_page: String,
}

impl Offer {
    /// Validated newer stable version of this client target.
    #[must_use]
    pub const fn version(&self) -> Version {
        self.version
    }

    /// Distinguish a package from source-build instructions; neither installs an update.
    #[must_use]
    pub const fn method(&self) -> Method {
        self.method
    }

    /// Canonical official release page constructed by the shared feed model.
    #[must_use]
    pub fn release_page(&self) -> &str {
        &self.release_page
    }
}

/// Truthful target state, independent of language and profile selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Availability {
    /// No trusted cache; never claim that the installed version is current.
    NeverChecked,
    /// The validated feed has no publication for this client target.
    NotPublished,
    /// The installed version is at least the validated published version.
    Current,
    /// A store target has no approved app-pinned listing yet.
    DestinationUnavailable,
    /// A newer package or source target with its fixed release information page.
    Available(Offer),
}

/// Compare only the installed client target against a trusted saved feed.
/// Feed authentication, delivery activation and failed-check status remain separate;
/// raw schema-valid bytes alone are not a trusted snapshot.
#[must_use]
pub fn project(snapshot: &Snapshot, platform: Platform, installed: Version) -> Availability {
    if snapshot.checkpoint().state.last_success.is_none() {
        return Availability::NeverChecked;
    }
    let Some(cache) = snapshot.cache() else {
        return Availability::NeverChecked;
    };
    let Some(release) = cache.feed().target(platform) else {
        return Availability::NotPublished;
    };
    if release.method() == Method::Store {
        return Availability::DestinationUnavailable;
    }
    if release.version() <= installed {
        return Availability::Current;
    }
    Availability::Available(Offer {
        version: release.version(),
        method: release.method(),
        release_page: release.release_page(),
    })
}

#[cfg(test)]
mod tests;
