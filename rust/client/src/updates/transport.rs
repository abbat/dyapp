//! Fixed-origin HTTPS feed transport. Invoke only on an IO worker after live-feed verification.
use super::feed::{self, Feed};
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

/// The approved endpoint; callers cannot supply a different host, path or query.
pub const FEED_URL: &str = "https://abbat.github.io/dyapp/updates/v1/stable.json";

/// Schema-valid cached bytes, not an independent signature or rollback proof.
#[derive(Clone, Debug)]
pub struct Cache {
    bytes: Arc<[u8]>,
    etag: Option<String>,
    parsed: Arc<Feed>,
}

impl Cache {
    /// Revalidate saved bytes and conditional-request metadata before reuse.
    ///
    /// # Errors
    /// Rejects malformed/oversized feeds or unsafe `ETag` syntax.
    pub fn restore(bytes: Vec<u8>, etag: Option<&str>) -> Result<Self, Error> {
        let etag = checked_etag(etag)?;
        let parsed = Arc::new(Feed::parse(&bytes)?);
        Ok(Self {
            bytes: bytes.into(),
            etag,
            parsed,
        })
    }

    /// Validated data; target compatibility and highest-version checks remain separate.
    #[must_use]
    pub fn feed(&self) -> &Feed {
        &self.parsed
    }

    /// Exact bytes for the installation-wide durable cache.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Validated HTTP metadata, containing no account or network configuration.
    #[must_use]
    pub fn etag(&self) -> Option<&str> {
        self.etag.as_deref()
    }
}

/// A failed check never substitutes a guessed current version for prior validated state.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// TLS, DNS, connection or request failure.
    #[error("update request failed: {0}")]
    Network(String),
    /// Redirects and every status other than 200/304 are refused.
    #[error("unexpected update response status: {0}")]
    Status(u16),
    /// A 304 cannot establish a first validated feed.
    #[error("update response requires a validated cache")]
    NoCache,
    /// Conditional request/response metadata is ambiguous or unsafe.
    #[error("invalid update ETag")]
    Etag,
    /// A body could not be read completely.
    #[error("update response read failed: {0}")]
    Read(#[from] std::io::Error),
    /// The existing shared feed validator rejected the complete body.
    #[error(transparent)]
    Feed(#[from] feed::Error),
}

/// No identity, credentials, bootstrap settings or arbitrary request parameters enter this client.
#[derive(Debug)]
pub struct Client {
    agent: ureq::Agent,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Configure certificate-verified HTTPS, no redirects/proxies and a bounded full-request budget.
    #[must_use]
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .http_status_as_error(false)
            .proxy(None)
            .user_agent("dyapp-client-updates")
            .accept("application/json")
            .accept_encoding("")
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .new_agent();
        Self { agent }
    }

    /// Perform one GET; a valid 304 reuses only an already schema-valid cache.
    ///
    /// # Errors
    /// Refuses transport/status/metadata errors, incomplete or oversized bodies and invalid feeds.
    pub fn fetch(&self, cached: Option<&Cache>) -> Result<Cache, Error> {
        let mut request = self.agent.get(FEED_URL);
        if let Some(etag) = cached.and_then(Cache::etag) {
            request = request.header("If-None-Match", etag);
        }
        let mut response = request
            .call()
            .map_err(|error| Error::Network(error.to_string()))?;
        let status = response.status().as_u16();
        if !matches!(status, 200 | 304) {
            return Err(Error::Status(status));
        }
        let etag = {
            let all = response.headers().get_all("etag");
            let mut values = all.iter();
            let value = values
                .next()
                .map(|value| value.to_str())
                .transpose()
                .map_err(|_| Error::Etag)?;
            if values.next().is_some() {
                return Err(Error::Etag);
            }
            value.map(str::to_owned)
        };
        admit_reply(
            status,
            etag.as_deref(),
            response.body_mut().as_reader(),
            cached,
        )
    }
}

fn admit_reply(
    status: u16,
    etag: Option<&str>,
    body: impl Read,
    cached: Option<&Cache>,
) -> Result<Cache, Error> {
    if !matches!(status, 200 | 304) {
        return Err(Error::Status(status));
    }
    let etag = checked_etag(etag)?;
    if status == 304 {
        let mut cache = cached.ok_or(Error::NoCache)?.clone();
        if etag.is_some() {
            cache.etag = etag;
        }
        return Ok(cache);
    }
    let bound = u64::try_from(feed::MAX_BYTES).map_err(|_| feed::Error::TooLarge)?;
    let mut bytes = Vec::new();
    body.take(bound + 1).read_to_end(&mut bytes)?;
    if bytes.len() > feed::MAX_BYTES {
        return Err(feed::Error::TooLarge.into());
    }
    Cache::restore(bytes, etag.as_deref())
}

fn checked_etag(value: Option<&str>) -> Result<Option<String>, Error> {
    let Some(value) = value else {
        return Ok(None);
    };
    let tag = value.strip_prefix("W/").unwrap_or(value);
    if value.len() > feed::MAX_BYTES
        || tag.len() < 2
        || !tag.starts_with('"')
        || !tag.ends_with('"')
    {
        return Err(Error::Etag);
    }
    if !tag[1..tag.len() - 1]
        .bytes()
        .all(|byte| matches!(byte, 0x21 | 0x23..=0x7e))
    {
        return Err(Error::Etag);
    }
    Ok(Some(value.to_owned()))
}

#[cfg(test)]
mod tests;
