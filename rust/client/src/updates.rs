//! Pure stable-version and update-check cadence rules. The app owns persistence and HTTP.

/// A stable release version, compared by numeric components.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Version {
    /// Major numeric component.
    pub major: u32,
    /// Minor numeric component.
    pub minor: u32,
    /// Patch numeric component.
    pub patch: u32,
}

impl Version {
    /// Parse exactly `MAJOR.MINOR.PATCH`, without leading zeros or suffixes.
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split('.');
        let major = component(parts.next()?)?;
        let minor = component(parts.next()?)?;
        let patch = component(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    /// Parse exactly `vMAJOR.MINOR.PATCH`.
    pub fn parse_tag(tag: &str) -> Option<Self> {
        Self::parse(tag.strip_prefix('v')?)
    }
}

fn component(value: &str) -> Option<u32> {
    if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
        return None;
    }
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

/// Installation-wide interval selected in Settings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Interval {
    /// Check every six hours.
    SixHours,
    /// Check every twelve hours.
    TwelveHours,
    #[default]
    /// Check once per day.
    Day,
    /// Check once per week.
    Week,
}

impl Interval {
    /// Duration of this interval in seconds.
    pub const fn seconds(self) -> u64 {
        match self {
            Self::SixHours => 6 * 60 * 60,
            Self::TwelveHours => 12 * 60 * 60,
            Self::Day => 24 * 60 * 60,
            Self::Week => 7 * 24 * 60 * 60,
        }
    }
}

const MIN_RETRY: u64 = 15 * 60;
const MAX_RETRY: u64 = 6 * 60 * 60;

/// Persisted wall-clock state; the caller records an attempt when it starts and clears failures
/// only after a validated 200 response or 304 with a validated cached feed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CheckState {
    /// Unix time of the last started request, if any.
    pub last_attempt: Option<u64>,
    /// Unix time of the last validated response, if any.
    pub last_success: Option<u64>,
    /// Failed checks since the last validated response.
    pub consecutive_failures: u32,
}

/// The event asking whether an update check should start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Trigger {
    /// App launch or return from suspension.
    LaunchOrResume,
    /// Scheduled interval timer.
    Timer,
    /// Network connectivity returned.
    NetworkRestored,
    /// User explicitly requested a check.
    Manual,
}

/// True when a check is due at Unix time `now`. `jitter` is a stable, local installation seed;
/// it only spreads timer requests and must never be sent to the feed host. In-flight request
/// coalescing and monotonic timers are the app's responsibility.
pub fn check_due(
    now: u64,
    interval: Interval,
    state: CheckState,
    jitter: u64,
    trigger: Trigger,
) -> bool {
    if trigger == Trigger::Manual {
        return true;
    }
    // A wall-clock rollback invalidates persisted future timestamps. Retry once now so a
    // far-future value cannot suppress checks for days or weeks.
    if state.last_attempt.is_some_and(|at| at > now)
        || state.last_success.is_some_and(|at| at > now)
    {
        return true;
    }
    if trigger == Trigger::NetworkRestored {
        return state
            .last_attempt
            .is_none_or(|at| now.saturating_sub(at) >= MIN_RETRY);
    }
    if state.consecutive_failures > 0 {
        let retry = MIN_RETRY
            .saturating_mul(1u64 << state.consecutive_failures.saturating_sub(1).min(6))
            .min(MAX_RETRY);
        return state
            .last_attempt
            .is_none_or(|at| now.saturating_sub(at) >= retry);
    }
    let success = match state.last_success {
        Some(success) => success,
        None => return true,
    };
    let period = interval.seconds();
    let due_after = if trigger == Trigger::Timer {
        let window = period / 24;
        period - window + jitter % (window + 1)
    } else {
        period
    };
    now.saturating_sub(success) >= due_after
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_versions_are_strict_and_numeric() {
        assert!(Version::parse("2.10.0") > Version::parse("2.9.99"));
        assert_eq!(Version::parse_tag("v0.0.1"), Version::parse("0.0.1"));
        for invalid in [
            "",
            "v1.2.3",
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "1.2.-3",
            "1.2.3-rc1",
            "1.2.3 ",
            "4294967296.0.0",
            "1.２.3",
        ] {
            assert_eq!(Version::parse(invalid), None, "{invalid}");
        }
        assert_eq!(Version::parse_tag("1.2.3"), None);
    }

    #[test]
    fn cadence_and_manual_checks() {
        let state = CheckState {
            last_success: Some(100),
            ..CheckState::default()
        };
        assert!(check_due(
            100,
            Interval::Day,
            CheckState::default(),
            0,
            Trigger::LaunchOrResume
        ));
        assert!(!check_due(
            100 + 86_399,
            Interval::Day,
            state,
            0,
            Trigger::LaunchOrResume
        ));
        assert!(check_due(
            100 + 86_400,
            Interval::Day,
            state,
            0,
            Trigger::LaunchOrResume
        ));
        assert!(check_due(
            100 + 82_800,
            Interval::Day,
            state,
            0,
            Trigger::Timer
        ));
        assert!(!check_due(
            100 + 82_799,
            Interval::Day,
            state,
            0,
            Trigger::Timer
        ));
        assert!(!check_due(
            100 + 82_800,
            Interval::Day,
            state,
            3_600,
            Trigger::Timer
        ));
        assert!(check_due(100, Interval::Week, state, 0, Trigger::Manual));
        assert_eq!(Interval::default(), Interval::Day);
    }

    #[test]
    fn failures_restore_and_clock_rollback() {
        let state = CheckState {
            last_attempt: Some(100),
            consecutive_failures: 1,
            ..CheckState::default()
        };
        assert!(!check_due(999, Interval::Day, state, 0, Trigger::Timer));
        assert!(check_due(1_000, Interval::Day, state, 0, Trigger::Timer));
        assert!(!check_due(
            999,
            Interval::Day,
            state,
            0,
            Trigger::NetworkRestored
        ));
        assert!(check_due(
            1_000,
            Interval::Day,
            state,
            0,
            Trigger::NetworkRestored
        ));
        let maxed = CheckState {
            consecutive_failures: u32::MAX,
            ..state
        };
        assert!(!check_due(
            100 + MAX_RETRY - 1,
            Interval::Day,
            maxed,
            0,
            Trigger::Timer
        ));
        assert!(check_due(
            100 + MAX_RETRY,
            Interval::Day,
            maxed,
            0,
            Trigger::Timer
        ));
        assert!(check_due(50, Interval::Day, state, 0, Trigger::Timer));
        assert!(check_due(
            50,
            Interval::Day,
            CheckState {
                last_success: Some(100),
                ..CheckState::default()
            },
            0,
            Trigger::LaunchOrResume
        ));
    }
}
