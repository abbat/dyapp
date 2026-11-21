//! Transport of the shared read projection; no adapter-owned freshness policy.
use super::MobileOwnDevice;
use dyapp_client::device_directory;

/// Outcome of the requested directory read, independent of row count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobileDeviceReadState {
    /// No network read was requested.
    Cached,
    /// Network, settings or readback was unavailable.
    Unavailable,
    /// No verified same-owner profile was returned.
    NoProfile,
    /// A verified same-owner profile was returned.
    Verified,
}

/// Profile actually supplying the displayed rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobileDeviceRowSource {
    /// Saved profile, including fallback after an older verified readback.
    Saved,
    /// Strictly newer verified remote profile.
    Remote,
}

/// Complete read-only directory projection; never revocation or enrollment authority.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MobileDeviceSnapshot {
    /// Public selectors in common-core order.
    pub devices: Vec<MobileOwnDevice>,
    /// Actual requested-read outcome.
    pub state: MobileDeviceReadState,
    /// Actual source used for the rows.
    pub source: MobileDeviceRowSource,
}

impl From<device_directory::Snapshot> for MobileDeviceSnapshot {
    fn from(snapshot: device_directory::Snapshot) -> Self {
        use device_directory::{ReadState, RowSource};
        Self {
            devices: snapshot
                .devices
                .into_iter()
                .map(|device| MobileOwnDevice {
                    key: device.key,
                    current: device.this,
                })
                .collect(),
            state: match snapshot.state {
                ReadState::Cached => MobileDeviceReadState::Cached,
                ReadState::Unavailable => MobileDeviceReadState::Unavailable,
                ReadState::NoProfile => MobileDeviceReadState::NoProfile,
                ReadState::Verified => MobileDeviceReadState::Verified,
            },
            source: match snapshot.source {
                RowSource::Saved => MobileDeviceRowSource::Saved,
                RowSource::Remote => MobileDeviceRowSource::Remote,
            },
        }
    }
}
