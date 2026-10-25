//! Public partial birth dates and age bounds; the caller supplies the UTC civil date.

use crate::Profile;

/// A valid Gregorian civil date with a four-digit year.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CivilDate {
    year: u32,
    month: u8,
    day: u8,
}

impl CivilDate {
    /// A valid date from year 1 through 9999.
    pub fn new(year: u32, month: u8, day: u8) -> Option<Self> {
        (year > 0 && year <= 9999 && day > 0 && day <= days_in_month(year, month)).then_some(Self {
            year,
            month,
            day,
        })
    }

    /// Numeric `YYYYMMDD` form for a search index or bound.
    pub fn yyyymmdd(self) -> u32 {
        self.year * 10_000 + u32::from(self.month) * 100 + u32::from(self.day)
    }

    fn age_on(self, today: Self) -> Option<u32> {
        if self > today {
            return None;
        }
        let anniversary = if self.month == 2 && self.day == 29 && !leap(today.year) {
            (3, 1)
        } else {
            (self.month, self.day)
        };
        Some(today.year - self.year - u32::from((today.month, today.day) < anniversary))
    }
}

/// Earliest and latest dates a published partial birth date could mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BirthBounds {
    /// First possible birth date.
    pub earliest: CivilDate,
    /// Last possible birth date.
    pub latest: CivilDate,
}

impl Profile {
    /// Bounds for `birth_date`; `None` when absent or malformed.
    pub fn birth_bounds(&self) -> Option<BirthBounds> {
        let text = self.birth_date.as_bytes();
        if !matches!(text.len(), 4 | 7 | 10)
            || text.iter().enumerate().any(|(i, byte)| {
                if i == 4 || i == 7 {
                    *byte != b'-'
                } else {
                    !byte.is_ascii_digit()
                }
            })
        {
            return None;
        }
        let year = self.birth_date[..4].parse().ok()?;
        let month = if text.len() >= 7 {
            self.birth_date[5..7].parse().ok()?
        } else {
            1
        };
        let day = if text.len() == 10 {
            self.birth_date[8..10].parse().ok()?
        } else {
            1
        };
        let earliest = CivilDate::new(year, month, day)?;
        let latest_month = if text.len() == 4 { 12 } else { month };
        let latest_day = if text.len() == 10 {
            day
        } else {
            days_in_month(year, latest_month)
        };
        Some(BirthBounds {
            earliest,
            latest: CivilDate::new(year, latest_month, latest_day)?,
        })
    }

    /// Youngest and oldest possible ages at `today`; absent or future dates are unknown.
    pub fn possible_age_range(&self, today: CivilDate) -> Option<(u32, u32)> {
        let bounds = self.birth_bounds()?;
        Some((bounds.latest.age_on(today)?, bounds.earliest.age_on(today)?))
    }
}

fn leap(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn days_in_month(year: u32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{verify, Error};
    use dyapp_identity::Identity;

    fn profile(date: &str) -> Profile {
        Profile {
            version: 1,
            birth_date: date.into(),
            ..Profile::default()
        }
    }

    #[test]
    fn partial_dates_have_real_calendar_bounds() {
        let check = |text, first, last| {
            let bounds = profile(text).birth_bounds().unwrap();
            assert_eq!(bounds.earliest.yyyymmdd(), first);
            assert_eq!(bounds.latest.yyyymmdd(), last);
        };
        check("2000", 20000101, 20001231);
        check("2000-02", 20000201, 20000229);
        check("2001-02", 20010201, 20010228);
        check("2000-02-29", 20000229, 20000229);
    }

    #[test]
    fn malformed_and_conflicting_dates_are_rejected() {
        let owner = Identity::generate();
        for date in [
            "0000",
            "2000-2",
            "2000-13",
            "2001-02-29",
            "2000-02-30",
            "2000-02-1",
            "２０00",
        ] {
            assert_eq!(
                verify(&profile(date).sign(&owner)),
                Err(Error::Invalid("invalid birth date"))
            );
        }
        let mut both = profile("2000");
        both.age = 26;
        assert_eq!(
            verify(&both.sign(&owner)),
            Err(Error::Invalid("age and birth date both published"))
        );
        let mut tombstone = profile("2000");
        tombstone.deleted = true;
        assert_eq!(
            verify(&tombstone.sign(&owner)),
            Err(Error::Invalid("tombstone carries data"))
        );
        assert!(verify(&profile("").sign(&owner)).is_ok());
        assert!(verify(&profile("2000-02-29").sign(&owner)).is_ok());
    }

    #[test]
    fn age_range_respects_unknown_day_and_leap_anniversary() {
        let today = CivilDate::new(2026, 10, 25).unwrap();
        assert_eq!(profile("2000").possible_age_range(today), Some((25, 26)));
        assert_eq!(profile("2000-10").possible_age_range(today), Some((25, 26)));
        assert_eq!(
            profile("2000-10-25").possible_age_range(today),
            Some((26, 26))
        );
        assert_eq!(profile("").possible_age_range(today), None);
        assert_eq!(profile("2027").possible_age_range(today), None);
        let leap = profile("2004-02-29");
        assert_eq!(
            leap.possible_age_range(CivilDate::new(2025, 2, 28).unwrap()),
            Some((20, 20))
        );
        assert_eq!(
            leap.possible_age_range(CivilDate::new(2025, 3, 1).unwrap()),
            Some((21, 21))
        );
    }
}
