//! Thin native conversion of categorical and selected-location search input.

use crate::{SearchCondition, SearchOp};
use std::collections::HashMap;

/// Form choices interpreted by the common client core.
#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct SearchChoiceInputs {
    /// Canonical scale keys and selected wire values; zero means unset.
    pub picked: HashMap<String, i64>,
    /// Country code rather than a translated label.
    pub country: String,
    /// Raw interest text.
    pub interest: String,
    /// An actual selected suggestion, never free-form place text.
    pub place: Option<u32>,
}

/// Convert native records and delegate all choice interpretation to the core.
#[uniffi::export]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned records and sequences across the native boundary"
)]
pub fn search_choice_conditions(
    offered: Vec<u32>,
    inputs: SearchChoiceInputs,
) -> Vec<SearchCondition> {
    let raw = dyapp_client::search_input::SearchChoiceInputs {
        picked: inputs.picked.into_iter().collect(),
        country: inputs.country,
        interest: inputs.interest,
        place: inputs.place,
    };
    raw.conditions(&offered)
        .into_iter()
        .map(|condition| SearchCondition {
            field: condition.field,
            op: SearchOp::Eq,
            ints: condition.ints,
            strings: condition.strings,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_choices_keep_core_order_and_payloads() {
        let inputs = SearchChoiceInputs {
            picked: [("gender".into(), 1)].into(),
            country: "\u{feff}DE ".into(),
            interest: " music ".into(),
            place: Some(7),
        };
        let actual = search_choice_conditions(vec![20, 19, 4, 36], inputs);
        assert_eq!(
            actual.iter().map(|value| value.field).collect::<Vec<_>>(),
            [36, 4, 19, 20]
        );
        assert_eq!(actual[0].ints, [1]);
        assert_eq!(actual[1].strings, ["DE"]);
        assert_eq!(actual[2].strings, ["music"]);
        assert_eq!(actual[3].strings, ["7"]);
        assert!(actual.iter().all(|value| value.op == SearchOp::Eq));
    }

    #[test]
    fn hidden_country_does_not_drop_an_offered_selected_place() {
        let inputs = SearchChoiceInputs {
            country: "DE".into(),
            place: Some(u32::MAX),
            ..SearchChoiceInputs::default()
        };
        assert_eq!(
            search_choice_conditions(vec![20], inputs),
            [SearchCondition {
                field: 20,
                op: SearchOp::Eq,
                ints: Vec::new(),
                strings: vec!["4294967295".into()],
            }]
        );
        assert_eq!(
            search_choice_conditions(Vec::new(), SearchChoiceInputs::default()),
            [] as [SearchCondition; 0]
        );
    }
}
