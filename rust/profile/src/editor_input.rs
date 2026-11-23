//! Profile editor interpretation shared by all client adapters.

use crate::{valid_height, valid_seeking, Gender};
use std::collections::HashMap;

/// Parse a height field; blank clears it and invalid values return `None`.
#[must_use]
pub fn parse_height(typed: &str) -> Option<HashMap<String, i32>> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Some(HashMap::new());
    }
    let height = typed.parse::<u32>().ok()?;
    if !valid_height(Some(height)) {
        return None;
    }
    Some(HashMap::from([(
        "height".into(),
        i32::try_from(height).ok()?,
    )]))
}

/// Normalized seeking fields ready for an editable profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seeking {
    /// Known gender codes in ascending order.
    pub genders: Vec<u32>,
    /// Optional inclusive minimum partner age.
    pub age_from: Option<u32>,
    /// Optional inclusive maximum partner age.
    pub age_to: Option<u32>,
    /// True clears all the other seeking fields.
    pub not_looking: bool,
}

/// Check editor selections using core range rules and only known gender codes.
#[must_use]
pub fn valid_editor_seeking(genders: &[u32], from: Option<u32>, to: Option<u32>) -> bool {
    valid_seeking(genders, from, to)
        && genders
            .iter()
            .all(|&code| Gender::from_code(code).is_some())
}

fn age(typed: &str) -> Result<Option<u32>, std::num::ParseIntError> {
    let typed = typed.trim();
    if typed.is_empty() {
        Ok(None)
    } else {
        typed.parse().map(Some)
    }
}

/// Interpret raw seeking input; not-looking ignores hidden input and clears it.
#[must_use]
pub fn parse_seeking(genders: &[u32], from: &str, to: &str, not_looking: bool) -> Option<Seeking> {
    if not_looking {
        return Some(Seeking {
            not_looking: true,
            ..Seeking::default()
        });
    }
    let from = age(from).ok()?;
    let to = age(to).ok()?;
    if !valid_editor_seeking(genders, from, to) {
        return None;
    }
    let mut genders = genders.to_vec();
    genders.sort_unstable();
    Some(Seeking {
        genders,
        age_from: from,
        age_to: to,
        not_looking: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_is_optional_and_uses_profile_bounds() {
        assert_eq!(parse_height("  "), Some(HashMap::new()));
        for value in ["50", " 272 "] {
            assert!(parse_height(value).is_some());
        }
        for value in ["49", "273", "-1", "0", "4294967296", "abc", "1.5"] {
            assert_eq!(parse_height(value), None);
        }
    }

    #[test]
    fn seeking_defaults_and_inclusive_bounds_are_preserved() {
        assert_eq!(parse_seeking(&[], "", "", false), Some(Seeking::default()));
        assert_eq!(
            parse_seeking(&[2, 1], " 18 ", "120", false),
            Some(Seeking {
                genders: vec![1, 2],
                age_from: Some(18),
                age_to: Some(120),
                not_looking: false
            })
        );
    }

    #[test]
    fn invalid_seeking_never_becomes_an_unrestricted_query() {
        for (from, to) in [
            ("17", ""),
            ("", "121"),
            ("30", "20"),
            ("wrong", ""),
            ("4294967296", ""),
        ] {
            assert_eq!(parse_seeking(&[1], from, to, false), None);
        }
        for genders in [vec![0], vec![999], vec![1, 1]] {
            assert_eq!(parse_seeking(&genders, "", "", false), None);
        }
    }

    #[test]
    fn not_looking_clears_even_malformed_hidden_fields() {
        assert_eq!(
            parse_seeking(&[999], "wrong", "-1", true),
            Some(Seeking {
                not_looking: true,
                ..Seeking::default()
            })
        );
    }
}
