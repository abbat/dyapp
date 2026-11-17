use super::*;

#[test]
fn a_height_filter_takes_either_bound_and_rechecks_inclusively() {
    let today = CivilDate::new(2026, 10, 25).unwrap();
    let keeps = |min_height, max_height, height| {
        let filter = Filter {
            min_height,
            max_height,
            ..Filter::default()
        };
        let profile = Profile {
            height,
            ..Profile::default()
        };
        let conditions = filter.conditions(today).unwrap();
        conditions.iter().all(|c| matches(c, &profile))
    };
    assert!(keeps(Some(170), None, Some(170)));
    assert!(!keeps(Some(170), None, Some(169)));
    assert!(keeps(None, Some(170), Some(170)));
    assert!(!keeps(None, Some(170), Some(171)));
    assert!(keeps(Some(170), Some(170), Some(170)));
    assert!(!keeps(Some(170), None, None));
    assert!(!keeps(None, Some(170), None));
    assert!(keeps(None, None, None));
}

fn assert_empty_filter_ranges() {
    let today = CivilDate::new(2026, 10, 25).unwrap();
    let open = |filter: Filter| filter.conditions(today).unwrap_err();
    let young = Filter {
        age: Some((10, 17)),
        ..Filter::default()
    };
    assert_eq!(open(young), FilterError::EmptyRange);
    let poor = Filter {
        country: "DE".into(),
        income: Some((2, 1)),
        ..Filter::default()
    };
    assert_eq!(open(poor), FilterError::EmptyRange);
    for (min_height, max_height) in [(Some(181), Some(180)), (Some(273), None), (None, Some(49))] {
        let height = Filter {
            min_height,
            max_height,
            ..Filter::default()
        };
        assert_eq!(open(height), FilterError::EmptyRange);
    }
    for min in [0, 15, 70] {
        let level = Filter {
            min_education: Some(min),
            ..Filter::default()
        };
        assert_eq!(open(level), FilterError::EmptyRange);
    }
}

fn assert_filter_scale_ranges() {
    let today = CivilDate::new(2026, 10, 25).unwrap();
    let open = |filter: Filter| filter.conditions(today).unwrap_err();
    for value in [0, Gender::MAX + 1] {
        assert_eq!(
            open(Filter {
                genders: vec![value],
                ..Filter::default()
            }),
            FilterError::InvalidScale
        );
    }
    assert_eq!(
        open(Filter {
            career_ambitions: Some(ScaleSelection::Range(10, 15)),
            ..Filter::default()
        }),
        FilterError::InvalidScale
    );
    assert_eq!(
        open(Filter {
            career_ambitions: Some(ScaleSelection::Range(30, 10)),
            ..Filter::default()
        }),
        FilterError::EmptyRange
    );
}

#[test]
fn a_filter_refuses_what_a_node_would_not_take() {
    let today = CivilDate::new(2026, 10, 25).unwrap();
    let open = |filter: Filter| filter.conditions(today).unwrap_err();
    let places = Filter {
        places: vec!["1".into()],
        ..Filter::default()
    };
    assert_eq!(open(places), FilterError::NeedsCountry);
    let income = Filter {
        income: Some((1, 2)),
        ..Filter::default()
    };
    assert_eq!(open(income), FilterError::NeedsCountry);
    assert_empty_filter_ranges();
    let many = Filter {
        country: "DE".into(),
        places: (0..17).map(|i| i.to_string()).collect(),
        ..Filter::default()
    };
    assert_eq!(open(many), FilterError::TooManyValues);
    let sixteen = Filter {
        places: (0..16).map(|i| i.to_string()).collect(),
        age: Some((18, 18)),
        country: "DE".into(),
        income: Some((1, 1)),
        ..Filter::default()
    };
    assert_eq!(sixteen.conditions(today).unwrap().len(), 5);
    assert_filter_scale_ranges();
}
