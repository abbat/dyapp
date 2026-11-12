//! The node's clock: the one place that reads wall and monotonic time, so tests can drive both.

use governor::nanos::Nanos;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A source of monotonic and wall time.
pub trait Source: Send + Sync {
    /// Monotonic time; never goes back.
    fn instant(&self) -> Instant;
    /// Wall time since the Unix epoch; may jump either way.
    fn unix(&self) -> Duration;
}

/// The operating system's clocks.
pub struct System;

impl Source for System {
    fn instant(&self) -> Instant {
        Instant::now()
    }

    fn unix(&self) -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
    }
}

/// A shared handle to a [`Source`]; also governor's clock for rate-limit buckets.
#[derive(Clone)]
pub struct Clock {
    source: Arc<dyn Source>,
    /// Governor's zero point.
    origin: Instant,
}

impl Clock {
    /// A handle to `source`.
    #[must_use]
    pub fn new(source: Arc<dyn Source>) -> Self {
        let origin = source.instant();
        Self { source, origin }
    }

    /// The operating system's clocks.
    #[must_use]
    pub fn system() -> Self {
        Self::new(Arc::new(System))
    }

    /// Monotonic time.
    #[must_use]
    pub fn instant(&self) -> Instant {
        self.source.instant()
    }

    /// Whole seconds of wall time since the Unix epoch.
    #[must_use]
    pub fn unix_secs(&self) -> u64 {
        self.source.unix().as_secs()
    }

    /// Signed whole seconds since the Unix epoch, as `chrono::Utc::now().timestamp()` gives.
    #[must_use]
    pub fn timestamp(&self) -> i64 {
        i64::try_from(self.unix_secs()).unwrap_or(i64::MAX)
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::system()
    }
}

impl governor::clock::Clock for Clock {
    type Instant = Nanos;

    fn now(&self) -> Nanos {
        Nanos::from(self.instant().saturating_duration_since(self.origin))
    }
}

/// A test clock that moves only when told to.
#[cfg(test)]
pub(crate) struct Manual {
    start: Instant,
    /// Monotonic time passed and wall time since the epoch.
    state: std::sync::Mutex<(Duration, Duration)>,
}

#[cfg(test)]
impl Manual {
    /// Starts at the real wall time rounded down to a whole second, so window edges are exact.
    pub(crate) fn new() -> Arc<Self> {
        Self::at(System.unix().as_secs())
    }

    /// Starts at `unix` seconds of wall time.
    pub(crate) fn at(unix: u64) -> Arc<Self> {
        Arc::new(Self {
            start: Instant::now(),
            state: std::sync::Mutex::new((Duration::ZERO, Duration::from_secs(unix))),
        })
    }

    /// A [`Clock`] reading this one.
    pub(crate) fn clock(self: &Arc<Self>) -> Clock {
        Clock::new(self.clone())
    }

    /// Moves both clocks forward by `by`.
    pub(crate) fn advance(&self, by: Duration) {
        let mut state = self.state.lock().unwrap();
        state.0 += by;
        state.1 += by;
    }

    /// Moves the wall clock alone, as an NTP step or an operator does; monotonic time stays.
    pub(crate) fn set_unix(&self, unix: u64) {
        self.state.lock().unwrap().1 = Duration::from_secs(unix);
    }
}

#[cfg(test)]
impl Source for Manual {
    fn instant(&self) -> Instant {
        self.start + self.state.lock().unwrap().0
    }

    fn unix(&self) -> Duration {
        self.state.lock().unwrap().1
    }
}
