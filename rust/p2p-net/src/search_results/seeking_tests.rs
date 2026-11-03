//! Who a profile seeks: the mutual conditions and their client re-check.
use super::*;

fn today() -> CivilDate {
    CivilDate::new(2026, 10, 25).unwrap()
}

fn asks(gender: u32, age: u32) -> Vec<Condition> {
    let filter = Filter {
        seeks: Some((gender, age)),
        ..Filter::default()
    };
    filter.conditions(today()).unwrap()
}

fn seeker(genders: &[u32], from: Option<u32>, to: Option<u32>) -> Profile {
    Profile {
        version: 1,
        seeking_genders: genders.to_vec(),
        seeking_age_from: from,
        seeking_age_to: to,
        ..Profile::default()
    }
}

#[test]
fn a_mutual_search_asks_for_the_gender_and_the_age() {
    let conditions = asks(Gender::Man.code(), 30);
    assert_eq!(conditions.len(), 2);
    assert_eq!(conditions[0].field, SEEKING_GENDERS);
    assert_eq!(conditions[0].ints, [i64::from(Gender::Man.code())]);
    assert_eq!(conditions[1].field, SEEKING_AGE);
    assert_eq!(conditions[1].ints, [30]);
    assert!(conditions.iter().all(|c| c.op == i32::from(Op::Eq)));
    assert!(checkable(SEEKING_GENDERS) && checkable(SEEKING_AGE));
}

#[test]
fn a_result_must_seek_the_gender_and_allow_the_age() {
    let (w, m) = (Gender::Woman.code(), Gender::Man.code());
    let bounded = seeker(&[w, m], Some(25), Some(35));
    for (gender, age, expected) in [
        (w, 25, true),
        (m, 35, true),
        (w, 24, false),
        (w, 36, false),
        (Gender::Other.code(), 30, false),
    ] {
        assert_eq!(
            passes(&asks(gender, age), 0, &bounded),
            expected,
            "{gender} {age}"
        );
    }
    let open_above = seeker(&[w], Some(25), None);
    assert!(passes(&asks(w, MAX_SEEKING_AGE), 0, &open_above));
    assert!(!passes(&asks(w, 24), 0, &open_above));
    let open_below = seeker(&[w], None, Some(35));
    assert!(passes(&asks(w, MIN_SEEKING_AGE), 0, &open_below));
    assert!(!passes(&asks(w, 36), 0, &open_below));

    assert!(!passes(&asks(w, 30), 0, &seeker(&[w], None, None)));
    assert!(!passes(&asks(w, 30), 0, &seeker(&[], Some(25), Some(35))));
    let not_looking = Profile {
        version: 1,
        not_looking: true,
        ..Profile::default()
    };
    assert!(!passes(&asks(w, 30), 0, &not_looking));
    assert!(passes(&[], 0, &not_looking));
}

#[test]
fn a_filter_refuses_an_unknown_gender_or_an_age_nobody_can_seek() {
    for (gender, age) in [
        (0, 30),
        (99, 30),
        (1, MIN_SEEKING_AGE - 1),
        (1, MAX_SEEKING_AGE + 1),
    ] {
        let filter = Filter {
            seeks: Some((gender, age)),
            ..Filter::default()
        };
        assert_eq!(filter.conditions(today()), Err(FilterError::InvalidScale));
    }
    assert_eq!(asks(1, MIN_SEEKING_AGE).len(), 2);
    assert_eq!(asks(1, MAX_SEEKING_AGE).len(), 2);
}

#[test]
fn a_malformed_age_condition_matches_nothing_and_an_unspecified_one_everything() {
    let profile = seeker(&[1], Some(25), Some(35));
    let age = |op: Op, ints: Vec<i64>, strings: Vec<String>| Condition {
        field: SEEKING_AGE,
        op: op.into(),
        ints,
        strings,
    };
    for bad in [
        age(Op::In, vec![30], vec![]),
        age(Op::Eq, vec![30, 31], vec![]),
        age(Op::Eq, vec![], vec![]),
        age(Op::Eq, vec![30], vec!["30".into()]),
    ] {
        assert!(!matches(&bad, &profile), "{bad:?}");
    }
    assert!(matches(&age(Op::Unspecified, vec![], vec![]), &profile));
}

#[test]
fn mutual_conditions_need_a_known_gender_and_an_exact_adult_age() {
    let me = |gender, month, day| Profile {
        gender,
        birth_year: Some(1990),
        birth_month: month,
        birth_day: day,
        ..Profile::default()
    };
    let man = Gender::Man.code();
    let conditions = mutual_conditions(&me(man, Some(1), Some(1)), today()).unwrap();
    assert_eq!(conditions, asks(man, 36));
    assert_eq!(mutual_conditions(&me(0, Some(1), Some(1)), today()), None);
    // Born in October 1990: 35 or 36 on 2026-10-25, depending on the day.
    assert_eq!(mutual_conditions(&me(man, Some(10), None), today()), None);
    assert_eq!(mutual_conditions(&Profile::default(), today()), None);
    let minor = Profile {
        birth_year: Some(2020),
        ..me(man, Some(1), Some(1))
    };
    assert_eq!(mutual_conditions(&minor, today()), None);
}
