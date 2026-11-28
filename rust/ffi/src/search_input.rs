//! Native record conversion for the shared raw search range rules.

use crate::profile_publish::today;
use crate::{CoreError, SearchCondition, SearchOp};
use dyapp_profile::CivilDate;

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
    dyapp_client::search_input::range_conditions(offered, &raw, date)
        .map_err(|error| CoreError::Failed(error.to_string()))
        .map(|conditions| {
            conditions
                .into_iter()
                .map(|condition| SearchCondition {
                    field: condition.field,
                    op: SearchOp::Range,
                    ints: condition.ints,
                    strings: condition.strings,
                })
                .collect()
        })
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

/// Build the native search form's conditions in the shared range-first order.
///
/// # Errors
/// Reports the first invalid offered range or interest, an unsupported date,
/// or an unsupported comparison in the resulting conditions.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned records and sequences across the native boundary"
)]
pub fn search_filter_conditions(
    offered: Vec<u32>,
    ranges: SearchRangeInputs,
    choices: crate::SearchChoiceInputs,
) -> Result<Vec<SearchCondition>, CoreError> {
    filter_conditions_at(&offered, ranges, choices, today()?)
}

fn filter_conditions_at(
    offered: &[u32],
    ranges: SearchRangeInputs,
    choices: crate::SearchChoiceInputs,
    date: CivilDate,
) -> Result<Vec<SearchCondition>, CoreError> {
    use dyapp_p2p_net::proto::Op;

    let ranges = dyapp_profile::search_input::SearchRangeInputs {
        age_min: ranges.age_min,
        age_max: ranges.age_max,
        height_min: ranges.height_min,
        height_max: ranges.height_max,
    };
    let conditions =
        dyapp_client::search_input::filter_conditions(offered, &ranges, &choices.into(), date)
            .map_err(|error| CoreError::Failed(error.to_string()))?;
    conditions
        .into_iter()
        .map(|condition| {
            let op = match condition.op() {
                Op::Eq => SearchOp::Eq,
                Op::In => SearchOp::In,
                Op::Range => SearchOp::Range,
                Op::Unspecified => return Err(CoreError::Failed("invalid search operator".into())),
            };
            Ok(SearchCondition {
                field: condition.field,
                op,
                ints: condition.ints,
                strings: condition.strings,
            })
        })
        .collect()
}

/// Project offered fields to displayable search controls using shared prerequisites.
#[must_use]
#[uniffi::export]
pub fn search_visible_fields(offered: Vec<u32>) -> Vec<u32> {
    dyapp_client::search_input::visible_filter_fields(offered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_p2p_net::search_results::Filter;

    fn date() -> CivilDate {
        CivilDate::new(2026, 11, 10).expect("fixed valid date")
    }

    #[test]
    fn native_filter_returns_range_then_choice_with_wire_values() {
        let ranges = SearchRangeInputs {
            height_min: "160".into(),
            height_max: "180".into(),
            ..SearchRangeInputs::default()
        };
        let choices = crate::SearchChoiceInputs {
            country: "DE".into(),
            ..crate::SearchChoiceInputs::default()
        };
        assert_eq!(
            search_filter_conditions(vec![4, 46], ranges, choices).unwrap(),
            vec![
                SearchCondition {
                    field: 46,
                    op: SearchOp::Range,
                    ints: vec![160, 180],
                    strings: vec![],
                },
                SearchCondition {
                    field: 4,
                    op: SearchOp::Eq,
                    ints: vec![],
                    strings: vec!["DE".into()],
                },
            ]
        );
    }

    #[test]
    fn native_filter_reports_invalid_range_before_invalid_interest() {
        let ranges = SearchRangeInputs {
            age_min: "invalid".into(),
            ..SearchRangeInputs::default()
        };
        let choices = crate::SearchChoiceInputs {
            interest: "music\nother".into(),
            ..crate::SearchChoiceInputs::default()
        };
        let offered = [1001, 19];
        assert_eq!(
            filter_conditions_at(&offered, ranges, choices.clone(), date())
                .unwrap_err()
                .to_string(),
            "invalid search age"
        );
        assert_eq!(
            filter_conditions_at(&offered, SearchRangeInputs::default(), choices, date())
                .unwrap_err()
                .to_string(),
            "invalid search interest"
        );
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
