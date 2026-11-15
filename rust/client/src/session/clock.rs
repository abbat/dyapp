//! Session wall-clock conversion must fail before staging rather than wrap timestamps.
use super::Error;

pub(super) fn millis(seconds: u64) -> Result<u64, Error> {
    seconds
        .checked_mul(1_000)
        .ok_or(Error::Store(crate::store::Error::ClockRange))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_to_milliseconds_preserves_inclusive_integer_bounds() {
        assert_eq!(millis(0).unwrap(), 0);
        assert_eq!(millis(1).unwrap(), 1_000);
        assert_eq!(
            millis(18_446_744_073_709_551).unwrap(),
            18_446_744_073_709_551_000
        );
        for seconds in [18_446_744_073_709_552, u64::MAX] {
            assert!(matches!(
                millis(seconds),
                Err(Error::Store(crate::store::Error::ClockRange))
            ));
        }
    }
}
