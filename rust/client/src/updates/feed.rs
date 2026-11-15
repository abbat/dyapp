//! Bounded, data-only stable feed parsing. This authenticates neither origin nor publisher.

use super::Version;
use serde::de::{self, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Maximum complete HTTP response body admitted by the update feed contract.
pub const MAX_BYTES: usize = 64 * 1024;

/// Client targets understood by schema 1.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Platform {
    /// Linux amd64 package.
    LinuxX86_64,
    /// Windows amd64 package.
    WindowsX86_64,
    /// Universal macOS client.
    MacosUniversal,
    /// Android client.
    Android,
    /// iOS client.
    Ios,
}

impl Platform {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "linux-x86_64" => Some(Self::LinuxX86_64),
            "windows-x86_64" => Some(Self::WindowsX86_64),
            "macos-universal" => Some(Self::MacosUniversal),
            "android" => Some(Self::Android),
            "ios" => Some(Self::Ios),
            _ => None,
        }
    }
}

/// Delivery advertised by the publisher; availability does not verify an installer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    /// Open the official release page and verification instructions.
    Package,
    /// Requires a separately verified, app-pinned store listing.
    Store,
    /// Open source/signing instructions, never claim an installable binary.
    BuildFromSource,
}

impl Method {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "package" => Some(Self::Package),
            "store" => Some(Self::Store),
            "build-from-source" => Some(Self::BuildFromSource),
            _ => None,
        }
    }
}

/// A schema-valid target release with an invariant matching version and tag.
#[derive(Debug)]
pub struct Release {
    version: Version,
    method: Method,
}

impl Release {
    /// The stable numeric version.
    #[must_use]
    pub const fn version(&self) -> Version {
        self.version
    }

    /// Delivery mode; the platform must separately authorize its destination.
    #[must_use]
    pub const fn method(&self) -> Method {
        self.method
    }

    /// Canonical release tag, constructed from validated numeric components.
    #[must_use]
    pub fn tag(&self) -> String {
        let Version {
            major,
            minor,
            patch,
        } = self.version;
        format!("v{major}.{minor}.{patch}")
    }

    /// The fixed official information page; this is not an installer or a store destination.
    #[must_use]
    pub fn release_page(&self) -> String {
        format!("https://github.com/abbat/dyapp/releases/tag/{}", self.tag())
    }
}

/// Schema-valid metadata only. Transport authentication and rollback state remain separate.
#[derive(Debug)]
pub struct Feed {
    targets: BTreeMap<Platform, Release>,
}

/// Why bytes cannot become a schema-valid feed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    /// The complete response body exceeded the fixed bound.
    #[error("update feed too large")]
    TooLarge,
    /// Invalid JSON, unsupported schema/channel or malformed target metadata.
    #[error("invalid update feed")]
    Invalid,
}

impl Feed {
    /// Parse the complete bounded response without network, storage or notification effects.
    ///
    /// # Errors
    /// Rejects oversized bytes, unsupported schema/channel, duplicate targets/fields, unknown
    /// known-target fields/methods, and noncanonical or mismatched versions/tags.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::TooLarge);
        }
        let raw: RawFeed = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        if raw.schema != 1 || raw.channel != "stable" {
            return Err(Error::Invalid);
        }
        let mut targets = BTreeMap::new();
        for (platform, raw) in raw.targets {
            let version = Version::parse(&raw.version).ok_or(Error::Invalid)?;
            if Version::parse_tag(&raw.tag) != Some(version) {
                return Err(Error::Invalid);
            }
            let method = Method::parse(&raw.method).ok_or(Error::Invalid)?;
            targets.insert(platform, Release { version, method });
        }
        Ok(Self { targets })
    }

    /// Missing target means not published, never proof that the installed version is current.
    #[must_use]
    pub fn target(&self, platform: Platform) -> Option<&Release> {
        self.targets.get(&platform)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFeed {
    schema: u32,
    channel: String,
    #[serde(deserialize_with = "targets")]
    targets: BTreeMap<Platform, RawRelease>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRelease {
    version: String,
    tag: String,
    method: String,
}

fn targets<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<Platform, RawRelease>, D::Error> {
    struct Targets;

    impl<'de> Visitor<'de> for Targets {
        type Value = BTreeMap<Platform, RawRelease>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an object of unique platform targets")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut targets = BTreeMap::new();
            let mut seen = BTreeSet::new();
            while let Some(key) = map.next_key::<String>()? {
                if !seen.insert(key.clone()) {
                    return Err(de::Error::custom("duplicate update target"));
                }
                if let Some(platform) = Platform::parse(&key) {
                    targets.insert(platform, map.next_value::<RawRelease>()?);
                } else {
                    // Future target schemas do not change what an older client can act upon.
                    map.next_value::<IgnoredAny>()?;
                }
            }
            Ok(targets)
        }
    }

    deserializer.deserialize_map(Targets)
}

#[cfg(test)]
mod tests;
