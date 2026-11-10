//! Raw editor birth parts validated against the same rule as published profiles.

use crate::Profile;
use std::collections::HashMap;

/// Independently optional birth parts keyed by their profile field names.
pub type BirthPartValues = HashMap<String, i32>;

/// Parse optional raw editor values; blank parts stay absent and invalid input returns `None`.
#[must_use]
pub fn parse_birth_parts(year: &str, month: &str, day: &str) -> Option<BirthPartValues> {
    let mut values = BirthPartValues::new();
    for (key, text) in [
        ("birth_year", year),
        ("birth_month", month),
        ("birth_day", day),
    ] {
        let text = text.trim();
        if !text.is_empty() {
            values.insert(key.into(), text.parse().ok()?);
        }
    }
    valid_birth_parts(&values).then_some(values)
}

/// Validate only recognized birth fields, rejecting negative values and impossible partial dates.
#[must_use]
pub fn valid_birth_parts(values: &BirthPartValues) -> bool {
    let mut profile = Profile::default();
    for (key, value) in values {
        let Ok(value) = u32::try_from(*value) else {
            return false;
        };
        match key.as_str() {
            "birth_year" => profile.birth_year = Some(value),
            "birth_month" => profile.birth_month = Some(value),
            "birth_day" => profile.birth_day = Some(value),
            _ => return false,
        }
    }
    profile.valid_birth_parts()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_parts_stay_absent_and_yearless_leap_day_is_valid() {
        assert_eq!(parse_birth_parts("", "", ""), Some(BirthPartValues::new()));
        let values = parse_birth_parts("", " 2 ", "29").expect("yearless leap day");
        assert_eq!(values.len(), 2);
        assert_eq!(values.get("birth_month"), Some(&2));
        assert_eq!(values.get("birth_day"), Some(&29));
        assert_eq!(
            parse_birth_parts("2000", "", "").expect("year only").len(),
            1
        );
        assert!(parse_birth_parts("2024", "2", "29").is_some());
    }

    #[test]
    fn impossible_dates_and_invalid_numbers_are_rejected() {
        for (year, month, day) in [
            ("2023", "2", "29"),
            ("", "4", "31"),
            ("0", "", ""),
            ("", "13", ""),
            ("", "2", "not a day"),
            ("-1", "", ""),
            ("2147483648", "", ""),
            ("10000", "", ""),
        ] {
            assert_eq!(parse_birth_parts(year, month, day), None);
        }
        assert!(!valid_birth_parts(&HashMap::from([("unknown".into(), 1)])));
        assert!(!valid_birth_parts(&HashMap::from([(
            "birth_day".into(),
            -1
        )])));
    }
}
