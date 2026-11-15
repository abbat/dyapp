//! Monotonic app-session locking; platforms supply events and schedule the returned deadline.

const BACKGROUND_GRACE_MS: u64 = 60_000;
const FOREGROUND_IDLE_MS: u64 = 300_000;

/// Platform lifecycle or successfully authorized credential event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Cover the app-switcher snapshot immediately and start the background grace.
    Background,
    /// Return to foreground without reviving an expired session.
    Foreground,
    /// Actual foreground interaction, applied only after checking expiration.
    Interaction,
    /// Successful PIN or OS authentication while foregrounded.
    Unlocked,
    /// Successful enrollment or authorized switch to a protected mode.
    Enrolled,
    /// Authorized removal of the app lock.
    Cleared,
}

/// Current session policy; contains no credential, identity or wall-clock time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// A PIN or platform authentication protects this session.
    pub enabled: bool,
    /// Private content must remain covered until successful authentication.
    pub locked: bool,
    /// The platform must cover its background snapshot.
    pub obscured: bool,
    /// Remaining monotonic milliseconds, absent while disabled or locked.
    pub remaining_ms: Option<u64>,
}

/// Invalid lifecycle input, without changing a failed authentication into success.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Successful authentication cannot unlock a background or disabled app.
    #[error("Unlock requires a foreground app PIN")]
    NotForeground,
    /// The supplied monotonic clock moved backwards.
    #[error("App lock monotonic clock moved backwards")]
    ClockWentBackwards,
}

/// Ephemeral app-session timer, independent of persisted PIN records and selected profiles.
pub struct Timer {
    enabled: bool,
    locked: bool,
    background_at: Option<u64>,
    interaction_at: u64,
    observed_at: u64,
}

impl Timer {
    /// Start a fresh process locked whenever a protected mode exists.
    #[must_use]
    pub fn new(enabled: bool, now_ms: u64) -> Self {
        Self {
            enabled,
            locked: enabled,
            background_at: None,
            interaction_at: now_ms,
            observed_at: now_ms,
        }
    }

    /// Apply expiration before a platform event, so late input cannot revive an expired session.
    ///
    /// # Errors
    /// Rejects backwards clocks and unlock without enabled foreground protection.
    pub fn apply(&mut self, event: Event, now_ms: u64) -> Result<Snapshot, Error> {
        self.observe(now_ms)?;
        match event {
            Event::Background => self.background_at = Some(now_ms),
            Event::Foreground => {
                self.background_at = None;
                self.interaction_at = now_ms;
            }
            // Background grace uses background_at; recording input never clears the lock.
            Event::Interaction => self.interaction_at = now_ms,
            Event::Unlocked => {
                if !self.enabled || self.background_at.is_some() {
                    return Err(Error::NotForeground);
                }
                self.locked = false;
                self.interaction_at = now_ms;
            }
            Event::Enrolled => {
                self.enabled = true;
                self.locked = false;
                self.interaction_at = now_ms;
            }
            Event::Cleared => {
                self.enabled = false;
                self.locked = false;
            }
        }
        Ok(self.snapshot(now_ms))
    }

    /// Observe the current deadline; an invalid clock covers protected content immediately.
    #[must_use]
    pub fn snapshot(&mut self, now_ms: u64) -> Snapshot {
        let _ = self.observe(now_ms);
        let remaining_ms = if self.enabled && !self.locked {
            now_ms
                .checked_sub(self.start())
                .and_then(|elapsed| self.limit().checked_sub(elapsed))
        } else {
            None
        };
        Snapshot {
            enabled: self.enabled,
            locked: self.locked,
            obscured: self.background_at.is_some(),
            remaining_ms,
        }
    }

    fn observe(&mut self, now_ms: u64) -> Result<(), Error> {
        if now_ms < self.observed_at {
            self.locked = self.enabled;
            return Err(Error::ClockWentBackwards);
        }
        self.observed_at = now_ms;
        if self.enabled
            && now_ms
                .checked_sub(self.start())
                .is_none_or(|elapsed| elapsed >= self.limit())
        {
            self.locked = true;
        }
        Ok(())
    }

    fn start(&self) -> u64 {
        self.background_at.unwrap_or(self.interaction_at)
    }

    fn limit(&self) -> u64 {
        if self.background_at.is_some() {
            BACKGROUND_GRACE_MS
        } else {
            FOREGROUND_IDLE_MS
        }
    }
}

#[cfg(test)]
mod tests;
