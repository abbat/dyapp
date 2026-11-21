use super::*;

#[test]
fn canonical_interest_queries_match_actual_profile_tags() {
    for typed in ["  E\u{301}COLE  ", "\u{feff}\u{c9}COLE\u{a0}"] {
        let inputs = SearchChoiceInputs {
            interest: typed.into(),
            ..SearchChoiceInputs::default()
        };
        let conditions = inputs.checked_conditions(&[19]).unwrap();
        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].field, 19);
        assert_eq!(conditions[0].strings, ["\u{e9}cole"]);
        let profile = dyapp_profile::Profile {
            interests: crate::profile_editor::normalize_interests(&[" \u{c9}COLE ".into()])
                .unwrap(),
            ..dyapp_profile::Profile::default()
        };
        assert!(dyapp_p2p_net::search_results::passes(
            &conditions,
            0,
            &profile
        ));
        assert!(!dyapp_p2p_net::search_results::passes(
            &conditions,
            0,
            &dyapp_profile::Profile::default()
        ));
    }
}

#[test]
fn invalid_offered_interest_refuses_the_whole_query_but_hidden_text_is_ignored() {
    for typed in [
        "x".repeat(dyapp_profile::MAX_FIELD_CHARS.saturating_add(1)),
        "music\nother".into(),
        "mu\u{200b}sic".into(),
        "mu\0sic".into(),
    ] {
        let inputs = SearchChoiceInputs {
            country: "DE".into(),
            interest: typed,
            ..SearchChoiceInputs::default()
        };
        assert_eq!(
            inputs.checked_conditions(&[4, 19]),
            Err(crate::profile_editor::DraftError::Interests)
        );
        assert_eq!(
            inputs.checked_conditions(&[4]).unwrap(),
            inputs.conditions(&[4])
        );
    }
}

#[test]
fn canonical_blank_and_punctuation_keep_existing_query_semantics() {
    for typed in ["", " \n\t ", "\u{feff}", "\u{85}"] {
        let inputs = SearchChoiceInputs {
            interest: typed.into(),
            ..SearchChoiceInputs::default()
        };
        assert!(inputs.checked_conditions(&[19]).unwrap().is_empty());
    }
    let inputs = SearchChoiceInputs {
        interest: " Tea, Coffee; Music ".into(),
        ..SearchChoiceInputs::default()
    };
    let conditions = inputs.checked_conditions(&[19]).unwrap();
    assert_eq!(conditions[0].strings, ["tea, coffee; music"]);
}
