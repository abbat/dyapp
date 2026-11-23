use dyapp_client::updates as core;

/// Installed client target; never a server release or another client platform.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileUpdatePlatform {
    /// Linux amd64 client.
    LinuxX86_64,
    /// Windows amd64 client.
    WindowsX86_64,
    /// Universal macOS client.
    MacosUniversal,
    /// Android client.
    Android,
    /// iOS client.
    Ios,
}

impl From<MobileUpdatePlatform> for core::feed::Platform {
    fn from(value: MobileUpdatePlatform) -> Self {
        match value {
            MobileUpdatePlatform::LinuxX86_64 => Self::LinuxX86_64,
            MobileUpdatePlatform::WindowsX86_64 => Self::WindowsX86_64,
            MobileUpdatePlatform::MacosUniversal => Self::MacosUniversal,
            MobileUpdatePlatform::Android => Self::Android,
            MobileUpdatePlatform::Ios => Self::Ios,
        }
    }
}

/// Installation-wide update cadence; version/timer policy stays in core.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileUpdateInterval {
    /// Six hours.
    SixHours,
    /// Twelve hours.
    TwelveHours,
    /// Twenty-four hours.
    Day,
    /// Seven days.
    Week,
}

impl From<MobileUpdateInterval> for core::Interval {
    fn from(value: MobileUpdateInterval) -> Self {
        match value {
            MobileUpdateInterval::SixHours => Self::SixHours,
            MobileUpdateInterval::TwelveHours => Self::TwelveHours,
            MobileUpdateInterval::Day => Self::Day,
            MobileUpdateInterval::Week => Self::Week,
        }
    }
}

impl From<core::Interval> for MobileUpdateInterval {
    fn from(value: core::Interval) -> Self {
        match value {
            core::Interval::SixHours => Self::SixHours,
            core::Interval::TwelveHours => Self::TwelveHours,
            core::Interval::Day => Self::Day,
            core::Interval::Week => Self::Week,
        }
    }
}

/// Published delivery semantics; no variant installs an update.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileUpdateDelivery {
    /// Official package information page.
    Package,
    /// Requires a separately approved pinned store listing.
    Store,
    /// Source-build and signing instructions.
    BuildFromSource,
}

impl From<core::feed::Method> for MobileUpdateDelivery {
    fn from(value: core::feed::Method) -> Self {
        match value {
            core::feed::Method::Package => Self::Package,
            core::feed::Method::Store => Self::Store,
            core::feed::Method::BuildFromSource => Self::BuildFromSource,
        }
    }
}

/// Core-computed availability, independent of localized wording and profile selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileUpdateAvailability {
    /// No successfully checked cache.
    NeverChecked,
    /// No publication for the installed target.
    NotPublished,
    /// Installed version is at least the verified publication.
    Current,
    /// A required pinned destination is unfinished.
    DestinationUnavailable,
    /// A newer target has a supported information destination.
    Available,
}

/// Read-only information constructed by core, never from a native or feed-supplied URL.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileUpdateOffer {
    /// Canonical stable version text.
    pub version: String,
    /// Package/source semantics for native copy and actions.
    pub delivery: MobileUpdateDelivery,
    /// Exact official release information URL.
    pub release_page: String,
}

impl From<&core::availability::Offer> for MobileUpdateOffer {
    fn from(value: &core::availability::Offer) -> Self {
        Self {
            version: value.version().to_string(),
            delivery: value.method().into(),
            release_page: value.release_page().to_owned(),
        }
    }
}

/// Installation metadata only; no cache bytes, identity data or local jitter leave the core.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct MobileUpdateState {
    /// Version of the native client library, matching the packaged application version.
    pub installed_version: String,
    /// Stored installation-wide cadence.
    pub interval: MobileUpdateInterval,
    /// Last attempt in Unix seconds, if one has been recorded.
    pub last_attempt: Option<u64>,
    /// Last validated response in Unix seconds, never a failed check timestamp.
    pub last_success: Option<u64>,
    /// Durable admission says a request is in flight or was interrupted before completion.
    pub in_flight: bool,
    /// Exact shared target-selection result.
    pub availability: MobileUpdateAvailability,
    /// Present only for a supported newer target.
    pub offer: Option<MobileUpdateOffer>,
}
