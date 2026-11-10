//! Raw search form input interpreted once at the native boundary.

use crate::profile_publish::today;
use crate::{CoreError, SearchCondition, SearchOp};
use dyapp_p2p_net::search_results::{Filter, BIRTH_AGE};
use dyapp_profile::{CivilDate, HEIGHT_FIELD};

/// Unparsed age and height fields; hidden fields never constrain a query.
#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct SearchRangeInputs {
    /// Inclusive minimum age; blank defaults to 18 when either age is entered.
    pub age_min: String,
    /// Inclusive maximum age; blank defaults to 150 when either age is entered.
    pub age_max: String,
    /// Inclusive minimum height in centimetres; blank leaves the bound open.
    pub height_min: String,
    /// Inclusive maximum height in centimetres; blank leaves the bound open.
    pub height_max: String,
}

fn invalid(field: &str) -> CoreError {
    CoreError::Failed(format!("invalid search {field}"))
}

fn conditions(
    offered: &[u32],
    inputs: &SearchRangeInputs,
    date: CivilDate,
) -> Result<Vec<SearchCondition>, CoreError> {
    let raw = dyapp_profile::search_input::SearchRangeInputs {
        age_min: inputs.age_min.clone(),
        age_max: inputs.age_max.clone(),
        height_min: inputs.height_min.clone(),
        height_max: inputs.height_max.clone(),
    };
    let ranges = raw
        .parse(
            offered.contains(&BIRTH_AGE),
            offered.contains(&HEIGHT_FIELD),
        )
        .map_err(|error| CoreError::Failed(error.to_string()))?;
    let mut out = Vec::new();
    let age = Filter {
        age: ranges.age,
        ..Filter::default()
    };
    append(&mut out, &age, date, "age")?;
    let height = Filter {
        min_height: ranges.min_height,
        max_height: ranges.max_height,
        ..Filter::default()
    };
    append(&mut out, &height, date, "height")?;
    Ok(out)
}

fn append(
    out: &mut Vec<SearchCondition>,
    filter: &Filter,
    date: CivilDate,
    field: &str,
) -> Result<(), CoreError> {
    out.extend(
        filter
            .conditions(date)
            .map_err(|_| invalid(field))?
            .into_iter()
            .map(|condition| SearchCondition {
                field: condition.field,
                op: SearchOp::Range,
                ints: condition.ints,
                strings: condition.strings,
            }),
    );
    Ok(())
}

/// Parse offered age and height fields using the current UTC date.
///
/// # Errors
/// Returns the existing field-specific failure for malformed or impossible bounds,
/// or an error when the current date cannot be represented.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned records and sequences across the native boundary"
)]
pub fn search_range_conditions(
    offered: Vec<u32>,
    inputs: SearchRangeInputs,
) -> Result<Vec<SearchCondition>, CoreError> {
    conditions(&offered, &inputs, today()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> CivilDate {
        CivilDate::new(2026, 11, 10).expect("fixed valid date")
    }

    #[test]
    fn blank_and_hidden_fields_add_no_conditions() {
        assert_eq!(
            conditions(&[1001, 46], &SearchRangeInputs::default(), date()).expect("blank inputs"),
            [] as [SearchCondition; 0]
        );
        let inputs = SearchRangeInputs {
            age_min: "invalid".into(),
            height_max: "invalid".into(),
            ..SearchRangeInputs::default()
        };
        assert_eq!(
            conditions(&[], &inputs, date()).expect("hidden inputs"),
            [] as [SearchCondition; 0]
        );
    }

    #[test]
    fn trimmed_bounds_use_the_existing_fixed_date_validator() {
        let inputs = SearchRangeInputs {
            age_min: " 21 ".into(),
            height_max: " 180 ".into(),
            ..SearchRangeInputs::default()
        };
        let actual = conditions(&[1001, 46], &inputs, date()).expect("valid inputs");
        let expected = Filter {
            age: Some((21, 150)),
            max_height: Some(180),
            ..Filter::default()
        }
        .conditions(date())
        .expect("valid filter");
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_eq!(actual.field, expected.field);
            assert_eq!(actual.ints, expected.ints);
            assert_eq!(actual.op, SearchOp::Range);
        }
    }

    #[test]
    fn desktop_whitespace_reaches_the_fixed_date_query_boundary() {
        let blank = SearchRangeInputs {
            age_min: "\u{feff}".into(),
            height_max: "\u{feff}".into(),
            ..SearchRangeInputs::default()
        };
        assert_eq!(
            conditions(&[1001, 46], &blank, date()).unwrap(),
            [] as [SearchCondition; 0]
        );
        let invalid = SearchRangeInputs {
            age_min: "\u{85}".into(),
            height_max: "\u{85}".into(),
            ..SearchRangeInputs::default()
        };
        for (field, message) in [(1001, "invalid search age"), (46, "invalid search height")] {
            assert_eq!(
                conditions(&[field], &invalid, date())
                    .unwrap_err()
                    .to_string(),
                message
            );
        }
    }

    #[test]
    fn invalid_numbers_and_impossible_ranges_keep_field_errors() {
        for (from, to) in [("wrong", ""), ("4294967296", ""), ("-1", ""), ("30", "20")] {
            let inputs = SearchRangeInputs {
                age_min: from.into(),
                age_max: to.into(),
                ..SearchRangeInputs::default()
            };
            let error = conditions(&[1001], &inputs, date()).expect_err("invalid age");
            assert!(error.to_string().contains("invalid search age"));
        }
        for (from, to) in [
            ("wrong", ""),
            ("4294967296", ""),
            ("-1", ""),
            ("200", "150"),
        ] {
            let inputs = SearchRangeInputs {
                height_min: from.into(),
                height_max: to.into(),
                ..SearchRangeInputs::default()
            };
            let error = conditions(&[46], &inputs, date()).expect_err("invalid height");
            assert!(error.to_string().contains("invalid search height"));
        }
    }
}
