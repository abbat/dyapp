//! One app-wide persistence unit; bytes are local state, never proof of publisher authenticity.
use super::coordinator::Checkpoint;
use super::feed::Platform;
use super::transport::Cache;
use super::Version;
use std::collections::BTreeMap;

mod record;

/// Bounded local encoding, including an escaped 64 KiB feed and validated HTTP metadata.
pub const MAX_BYTES: usize = 512 * 1024;

/// Cadence and cache/high-water state move together in the eventual durable transaction.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    checkpoint: Checkpoint,
    cache: Option<Cache>,
    highest: BTreeMap<Platform, Version>,
}

/// Local corruption or a candidate that cannot replace existing validated state.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid, oversized, unsupported or internally inconsistent checkpoint.
    #[error("invalid update checkpoint")]
    Invalid,
    /// At least one known target fell below its own previously accepted version.
    #[error("update version rollback")]
    Rollback,
    /// Existing bounded feed/cache validation failed.
    #[error(transparent)]
    Cache(#[from] super::transport::Error),
}

impl Snapshot {
    /// Current admission state, including whether the process stopped during a request.
    #[must_use]
    pub const fn checkpoint(&self) -> Checkpoint {
        self.checkpoint
    }

    /// Retain the coordinator result for the next atomic durable write.
    pub fn set_checkpoint(&mut self, checkpoint: Checkpoint) {
        self.checkpoint = checkpoint;
    }

    /// Exact validated cache for the next conditional request; no guessed empty feed.
    #[must_use]
    pub fn cache(&self) -> Option<&Cache> {
        self.cache.as_ref()
    }

    /// A missing target in the latest feed does not erase its prior high-water mark.
    #[must_use]
    pub fn highest(&self, platform: Platform) -> Option<Version> {
        self.highest.get(&platform).copied()
    }

    /// Admit the whole candidate before replacing cache or any platform high-water mark.
    ///
    /// # Errors
    /// Rejects rollback without changing the snapshot. Origin authentication remains transport's job.
    pub fn admit(&mut self, cache: Cache) -> Result<(), Error> {
        for platform in Platform::ALL {
            if let Some(release) = cache.feed().target(platform) {
                if self
                    .highest(platform)
                    .is_some_and(|seen| release.version() < seen)
                {
                    return Err(Error::Rollback);
                }
            }
        }
        for platform in Platform::ALL {
            if let Some(release) = cache.feed().target(platform) {
                self.highest.insert(platform, release.version());
            }
        }
        self.cache = Some(cache);
        Ok(())
    }

    /// Encode one bounded versioned record for atomic persistence by the shared IO layer.
    ///
    /// # Errors
    /// Refuses internally inconsistent state or an encoding beyond the fixed local bound.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let bytes = serde_json::to_vec(&record::value(self)?).map_err(|_| Error::Invalid)?;
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        Ok(bytes)
    }

    /// Revalidate saved cache, cadence format and all high-water relationships.
    ///
    /// # Errors
    /// Refuses corrupt/oversized records; errors never become a fresh default checkpoint.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        let saved: record::Saved = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        let snapshot = saved.into_snapshot()?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.checkpoint.in_flight && self.checkpoint.state.last_attempt.is_none() {
            return Err(Error::Invalid);
        }
        let Some(cache) = &self.cache else {
            return if self.highest.is_empty() && self.checkpoint.state.last_success.is_none() {
                Ok(())
            } else {
                Err(Error::Invalid)
            };
        };
        for platform in Platform::ALL {
            if let Some(release) = cache.feed().target(platform) {
                if self.highest(platform) != Some(release.version()) {
                    return Err(Error::Invalid);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
