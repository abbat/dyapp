use super::*;

fn text(field: u32, value: &str) -> proto::Condition {
    proto::Condition {
        field,
        op: proto::Op::Eq.into(),
        ints: Vec::new(),
        strings: vec![value.into()],
    }
}

#[test]
fn hidden_fields_unknown_keys_and_unset_choices_do_not_constrain_search() {
    let inputs = SearchChoiceInputs {
        picked: [("gender".into(), 1), ("unknown".into(), 7)].into(),
        country: "US".into(),
        interest: "music".into(),
        place: Some(123),
    };
    assert_eq!(inputs.conditions(&[]), [] as [proto::Condition; 0]);
    let inputs = SearchChoiceInputs {
        picked: [("gender".into(), 0), ("unknown".into(), 7)].into(),
        ..SearchChoiceInputs::default()
    };
    let offered: Vec<_> = dyapp_profile::SCALES
        .iter()
        .map(|scale| scale.field)
        .collect();
    assert_eq!(inputs.conditions(&offered), [] as [proto::Condition; 0]);
}

#[test]
fn categorical_conditions_follow_metadata_order_and_preserve_wire_values() {
    let inputs = SearchChoiceInputs {
        picked: [("orientation".into(), 2), ("gender".into(), 1)].into(),
        ..SearchChoiceInputs::default()
    };
    let scales = &dyapp_profile::SCALES[..2];
    let conditions = inputs.conditions(&[scales[1].field, scales[0].field]);
    assert_eq!(conditions.len(), 2);
    for (condition, (scale, code)) in conditions.iter().zip(scales.iter().zip([1, 2])) {
        assert_eq!(condition.field, scale.field);
        assert_eq!(condition.op, i32::from(proto::Op::Eq));
        assert_eq!(condition.ints, [code]);
        assert_eq!(condition.strings, [] as [String; 0]);
    }
    assert_eq!(
        inputs.conditions(&[scales[1].field]),
        [conditions[1].clone()]
    );
}

#[test]
fn selected_place_is_independent_of_country_and_never_uses_typed_place_text() {
    let inputs = SearchChoiceInputs {
        country: "US".into(),
        place: Some(123),
        ..SearchChoiceInputs::default()
    };
    assert_eq!(inputs.conditions(&[20]), [text(20, "123")]);
    assert_eq!(inputs.conditions(&[4]), [text(4, "US")]);
    assert_eq!(
        inputs.conditions(&[20, 4]),
        [text(4, "US"), text(20, "123")]
    );
    let inputs = SearchChoiceInputs::default();
    assert_eq!(inputs.conditions(&[20]), [] as [proto::Condition; 0]);
}

#[test]
fn country_and_interest_use_linux_whitespace_and_fixed_wire_order() {
    let inputs = SearchChoiceInputs {
        country: "\u{feff}US\u{a0}".into(),
        interest: "  live music  ".into(),
        place: Some(123),
        ..SearchChoiceInputs::default()
    };
    assert_eq!(
        inputs.conditions(&[20, 19, 4]),
        [text(4, "US"), text(19, "live music"), text(20, "123")]
    );
    let inputs = SearchChoiceInputs {
        country: "\u{feff}".into(),
        interest: "\u{85}".into(),
        ..SearchChoiceInputs::default()
    };
    assert_eq!(inputs.conditions(&[4, 19]), [text(19, "\u{85}")]);
}
