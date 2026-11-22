//! Date-explicit construction of numeric search conditions shared by every client adapter.

use dyapp_p2p_net::proto::Condition;
use dyapp_p2p_net::search_results::{Filter, BIRTH_AGE};
use dyapp_profile::search_input::{SearchInputError, SearchRangeInputs};
use dyapp_profile::{CivilDate, HEIGHT_FIELD};

/// Interpret offered raw ranges, then emit age followed by height in the existing wire order.
///
/// # Errors
/// Preserves field-specific parse errors before semantic validation, then age before height.
pub fn range_conditions(
    offered: &[u32],
    inputs: &SearchRangeInputs,
    date: CivilDate,
) -> Result<Vec<Condition>, SearchInputError> {
    let ranges = inputs.parse(
        offered.contains(&BIRTH_AGE),
        offered.contains(&HEIGHT_FIELD),
    )?;
    let mut conditions = Filter {
        age: ranges.age,
        ..Filter::default()
    }
    .conditions(date)
    .map_err(|_| SearchInputError::Age)?;
    conditions.extend(
        Filter {
            min_height: ranges.min_height,
            max_height: ranges.max_height,
            ..Filter::default()
        }
        .conditions(date)
        .map_err(|_| SearchInputError::Height)?,
    );
    Ok(conditions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_p2p_net::proto::Op;

    fn today() -> CivilDate {
        CivilDate::new(2026, 11, 10).unwrap()
    }

    fn inputs(age: (&str, &str), height: (&str, &str)) -> SearchRangeInputs {
        SearchRangeInputs {
            age_min: age.0.into(),
            age_max: age.1.into(),
            height_min: height.0.into(),
            height_max: height.1.into(),
        }
    }

    fn condition(field: u32, bounds: [i64; 2]) -> Condition {
        Condition {
            field,
            op: Op::Range.into(),
            ints: bounds.into(),
            strings: Vec::new(),
        }
    }

    #[test]
    fn absent_or_blank_fields_never_constrain_the_query() {
        assert_eq!(
            range_conditions(&[], &inputs(("wrong", "-1"), ("wrong", "-1")), today()),
            Ok(Vec::new())
        );
        assert_eq!(
            range_conditions(
                &[BIRTH_AGE, HEIGHT_FIELD],
                &SearchRangeInputs::default(),
                today()
            ),
            Ok(Vec::new())
        );
    }

    #[test]
    fn actual_date_cutoffs_and_height_bounds_keep_the_original_order() {
        assert_eq!(
            range_conditions(
                &[HEIGHT_FIELD, BIRTH_AGE, BIRTH_AGE],
                &inputs((" 21 ", "30"), ("170", "")),
                today()
            ),
            Ok(vec![
                // Birth-age range excludes the lower cutoff and includes the upper cutoff.
                condition(BIRTH_AGE, [19_951_110, 20_051_110]),
                condition(HEIGHT_FIELD, [170, 272])
            ])
        );
        assert_eq!(
            range_conditions(
                &[BIRTH_AGE, HEIGHT_FIELD],
                &inputs(("", "40"), ("", "180")),
                today()
            ),
            Ok(vec![
                condition(BIRTH_AGE, [19_851_110, 20_081_110]),
                condition(HEIGHT_FIELD, [50, 180])
            ])
        );
    }

    #[test]
    fn parse_failures_precede_semantic_failures_without_changing_the_field() {
        for (raw, expected) in [
            (inputs(("wrong", ""), ("wrong", "")), SearchInputError::Age),
            (
                inputs(("30", "20"), ("wrong", "")),
                SearchInputError::Height,
            ),
            (inputs(("30", "20"), ("200", "150")), SearchInputError::Age),
            (inputs(("", ""), ("200", "150")), SearchInputError::Height),
            (inputs(("4294967296", ""), ("", "")), SearchInputError::Age),
        ] {
            assert_eq!(
                range_conditions(&[BIRTH_AGE, HEIGHT_FIELD], &raw, today()),
                Err(expected)
            );
        }
    }

    #[test]
    fn calendar_edges_and_maximum_age_keep_the_existing_wire_bounds() {
        let leap = CivilDate::new(2024, 2, 29).unwrap();
        assert_eq!(
            range_conditions(&[BIRTH_AGE], &inputs(("18", "18"), ("", "")), leap),
            Ok(vec![condition(BIRTH_AGE, [20_050_228, 20_060_228])])
        );
        assert_eq!(
            range_conditions(
                &[BIRTH_AGE],
                &inputs(("18", "4294967295"), ("", "")),
                today()
            ),
            Ok(vec![condition(BIRTH_AGE, [0, 20_081_110])])
        );
        assert_eq!(
            range_conditions(
                &[BIRTH_AGE],
                &inputs(("18", "18"), ("", "")),
                CivilDate::new(1, 1, 1).unwrap()
            ),
            Err(SearchInputError::Age)
        );
    }
}
