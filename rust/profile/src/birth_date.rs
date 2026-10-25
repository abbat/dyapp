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
    /// Whether the independently optional parts can describe a Gregorian date.
    pub(crate) fn valid_birth_parts(&self) -> bool {
        let year = self.birth_year.unwrap_or(2000); // Leap year admits yearless February 29.
        if !(1..=9999).contains(&year)
            || self.birth_month.is_some_and(|m| !(1..=12).contains(&m))
            || self.birth_day.is_some_and(|d| !(1..=31).contains(&d))
        {
            return false;
        }
        (1..=12).any(|month| {
            self.birth_month.is_none_or(|selected| selected == month)
                && CivilDate::new(year, month as u8, self.birth_day.unwrap_or(1) as u8).is_some()
        })
    }

    /// First and last possible dates when a year is published; otherwise unknown.
    pub fn birth_bounds(&self) -> Option<BirthBounds> {
        if !self.valid_birth_parts() {
            return None;
        }
        let year = self.birth_year?;
        let mut earliest = None;
        let mut latest = None;
        for month in 1..=12 {
            if self.birth_month.is_some_and(|selected| selected != month) {
                continue;
            }
            let first_day = self.birth_day.unwrap_or(1) as u8;
            let last_day =
                self.birth_day
                    .unwrap_or(u32::from(days_in_month(year, month as u8))) as u8;
            if let (Some(first), Some(last)) = (
                CivilDate::new(year, month as u8, first_day),
                CivilDate::new(year, month as u8, last_day),
            ) {
                earliest.get_or_insert(first);
                latest = Some(last);
            }
        }
        Some(BirthBounds {
            earliest: earliest?,
            latest: latest?,
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

    fn profile(year: Option<u32>, month: Option<u32>, day: Option<u32>) -> Profile {
        Profile {
            version: 1,
            birth_year: year,
            birth_month: month,
            birth_day: day,
            ..Profile::default()
        }
    }

    #[test]
    fn independent_parts_have_real_calendar_bounds() {
        let check = |p: Profile, first, last| {
            let bounds = p.birth_bounds().unwrap();
            assert_eq!(bounds.earliest.yyyymmdd(), first);
            assert_eq!(bounds.latest.yyyymmdd(), last);
        };
        check(profile(Some(2000), None, None), 20000101, 20001231);
        check(profile(Some(2000), Some(2), None), 20000201, 20000229);
        check(profile(Some(2001), Some(2), None), 20010201, 20010228);
        check(profile(Some(2000), Some(2), Some(29)), 20000229, 20000229);
        check(profile(Some(2000), None, Some(31)), 20000131, 20001231);
        assert_eq!(profile(None, Some(2), Some(29)).birth_bounds(), None);
    }

    #[test]
    fn all_presence_combinations_round_trip() {
        let owner = Identity::generate();
        for mask in 0..8 {
            let p = profile(
                (mask & 1 != 0).then_some(2000),
                (mask & 2 != 0).then_some(2),
                (mask & 4 != 0).then_some(29),
            );
            assert_eq!(verify(&p.sign(&owner)).unwrap().profile, p);
            assert_eq!(p.birth_bounds().is_some(), p.birth_year.is_some());
        }
    }

    #[test]
    fn impossible_and_conflicting_parts_are_rejected() {
        let owner = Identity::generate();
        for (year, month, day) in [
            (Some(0), None, None),
            (Some(10000), None, None),
            (None, Some(0), None),
            (None, Some(13), None),
            (None, None, Some(0)),
            (None, None, Some(32)),
            (Some(2001), Some(2), Some(29)),
            (None, Some(2), Some(30)),
            (None, Some(4), Some(31)),
        ] {
            assert_eq!(
                verify(&profile(year, month, day).sign(&owner)),
                Err(Error::Invalid("invalid birth date"))
            );
        }
        let mut both = profile(None, Some(2), Some(29));
        both.age = 26;
        assert_eq!(
            verify(&both.sign(&owner)),
            Err(Error::Invalid("age and birth date both published"))
        );
        let mut tombstone = profile(Some(2000), None, None);
        tombstone.deleted = true;
        assert_eq!(
            verify(&tombstone.sign(&owner)),
            Err(Error::Invalid("tombstone carries data"))
        );
    }

    #[test]
    fn age_range_respects_unknown_parts_and_leap_anniversary() {
        let today = CivilDate::new(2026, 10, 25).unwrap();
        assert_eq!(
            profile(Some(2000), None, None).possible_age_range(today),
            Some((25, 26))
        );
        assert_eq!(
            profile(Some(2000), Some(10), None).possible_age_range(today),
            Some((25, 26))
        );
        assert_eq!(
            profile(Some(2000), None, Some(25)).possible_age_range(today),
            Some((25, 26))
        );
        assert_eq!(
            profile(Some(2000), Some(10), Some(25)).possible_age_range(today),
            Some((26, 26))
        );
        assert_eq!(
            profile(None, Some(10), Some(25)).possible_age_range(today),
            None
        );
        assert_eq!(
            profile(Some(2027), None, None).possible_age_range(today),
            None
        );
        let leap = profile(Some(2004), Some(2), Some(29));
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
