use super::*;

#[test]
fn checked_native_records_keep_order_and_canonical_payloads() {
    let inputs = SearchChoiceInputs {
        picked: [("gender".into(), 1)].into(),
        country: "DE".into(),
        interest: " E\u{301}COLE ".into(),
        place: Some(7),
    };
    assert_eq!(
        search_checked_choice_conditions(vec![20, 19, 4, 36], inputs.clone()).unwrap(),
        search_choice_conditions(vec![20, 19, 4, 36], inputs)
    );
}

#[test]
fn invalid_interest_is_a_search_field_error_only_when_offered() {
    let inputs = SearchChoiceInputs {
        country: "DE".into(),
        interest: "music\nother".into(),
        ..SearchChoiceInputs::default()
    };
    assert_eq!(
        search_checked_choice_conditions(vec![4, 19], inputs.clone())
            .unwrap_err()
            .to_string(),
        "invalid search interest"
    );
    assert_eq!(
        search_checked_choice_conditions(vec![4], inputs.clone()).unwrap(),
        search_choice_conditions(vec![4], inputs)
    );
}
