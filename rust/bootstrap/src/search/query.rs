use super::tests::{index, signed};
use super::*;
use dyapp_identity::Identity;
use dyapp_p2p_net::proto::{Condition, Op};
use std::collections::HashSet;

fn cond(field: u32, op: Op, ints: Vec<i64>, strings: Vec<&str>) -> Condition {
    Condition {
        field,
        op: op.into(),
        ints,
        strings: strings.into_iter().map(String::from).collect(),
    }
}

/// 600 profiles: places p0..p5 in "DE", ages 20..79 on 2026-10-25, interest "chess" on every 3rd, gender
/// "f" on every 2nd, education level `i % 7 * 10`, one photo on every 4th.
/// `GeoNames` IDs of Berlin, Hamburg, Munich, Cologne, Frankfurt and Stuttgart.
const SIX: [&str; 6] = [
    "2950159", "2911298", "2867714", "2886242", "2925533", "2825297",
];

/// Returns the index and the keys in SIX[0]/SIX[3].
fn population() -> (SearchIndex, std::collections::HashSet<[u8; 32]>) {
    for place in SIX {
        assert_eq!(crate::places::canonical("DE", place), place);
    }
    let index = index();
    let mut p03 = std::collections::HashSet::new();
    for i in 0..600i64 {
        let id = Identity::generate();
        let p = Profile {
            version: 1,
            birth_year: Some(2026 - u32::try_from(20 + i % 60).unwrap()),
            birth_month: Some(1),
            birth_day: Some(1),
            country: "DE".into(),
            place: SIX[(i % 6) as usize].into(),
            gender: if i % 2 == 0 {
                dyapp_profile::Gender::Woman.code()
            } else {
                dyapp_profile::Gender::Man.code()
            },
            education_level: u32::try_from(i % 7 * 10).unwrap(),
            photos: if i % 4 == 0 {
                vec![dyapp_profile::Photo::default()]
            } else {
                vec![]
            },
            interests: if i % 3 == 0 {
                vec!["chess".into()]
            } else {
                vec![]
            },
            ..Profile::default()
        };
        let (key, p, r) = signed(&id, p);
        index.put(&key, &p, &r, 0, true).unwrap();
        if i % 3 == 0 {
            p03.insert(key);
        }
    }
    (index, p03)
}

#[test]
fn virtual_age_condition_contains_the_whole_birth_interval() {
    let index = index();
    let mut keys = Vec::new();
    for (year, month, day) in [
        (Some(2000), None, None),
        (Some(2000), Some(2), None),
        (Some(2000), Some(2), Some(29)),
        (Some(2001), None, None),
        (None, None, None),
        (None, Some(2), Some(29)),
    ] {
        let profile = Profile {
            version: 1,
            birth_year: year,
            birth_month: month,
            birth_day: day,
            ..Profile::default()
        };
        let (key, p, record) = signed(&Identity::generate(), profile);
        index.put(&key, &p, &record, 0, true).unwrap();
        keys.push(key);
    }
    let matches = |low, high| {
        let query = [cond(BIRTH_AGE, Op::Range, vec![low, high], vec![])];
        let found = index.query(&query, 200, 10_000).unwrap().unwrap();
        assert_eq!(found.applied, vec![BIRTH_AGE]);
        found
            .hits
            .into_iter()
            .map(|hit| hit.0)
            .collect::<HashSet<_>>()
    };
    assert_eq!(
        matches(19_991_231, 20_001_231),
        keys[..3].iter().copied().collect::<HashSet<_>>()
    );
    assert_eq!(
        matches(20_000_101, 20_000_301),
        keys[1..3].iter().copied().collect::<HashSet<_>>()
    );
    assert_eq!(
        matches(20_000_201, 20_000_229),
        [keys[2]].into_iter().collect::<HashSet<_>>()
    );
}

#[test]
fn query_filters_least_education() {
    let (index, _) = population();
    let found = index
        .query(&[cond(27, Op::Range, vec![50, 60], vec![])], 200, 10_000)
        .unwrap()
        .unwrap();
    assert_eq!(found.applied, vec![27]);
    // i % 7 in {5, 6}: 85 + 85 of the 600 (i = 5, 12, ..., 593 and 6, ..., 594).
    assert_eq!(found.hits.len(), 170);
}

#[test]
fn query_matches_closed_scales_and_skips_unpublished_values() {
    let index = index();
    let filled = Profile {
        version: 1,
        gender: 4,
        orientation: 5,
        relationship_goal: 6,
        role_preference: 3,
        career_ambition: 30,
        financial_philosophy: 30,
        employment_status: 6,
        fitness_level: 40,
        zodiac: 12,
        smoking: 30,
        drinking: 30,
        tattoos: 30,
        marital_status: 3,
        pets: 2,
        ..Profile::default()
    };
    let (key, profile, record) = signed(&Identity::generate(), filled);
    index.put(&key, &profile, &record, 0, true).unwrap();
    let (empty_key, profile, record) = signed(
        &Identity::generate(),
        Profile {
            version: 1,
            ..Profile::default()
        },
    );
    index.put(&empty_key, &profile, &record, 0, true).unwrap();

    for scale in dyapp_profile::SCALES {
        let condition = cond(scale.field, Op::Eq, vec![i64::from(scale.max)], vec![]);
        let found = index.query(&[condition], 10, 100).unwrap().unwrap();
        assert_eq!(found.applied, vec![scale.field]);
        assert_eq!(found.hits, vec![(key, 1)], "{}", scale.key);
        assert!(index
            .query(&[cond(scale.field, Op::Eq, vec![0], vec![])], 10, 100)
            .unwrap()
            .is_err());
        let range = cond(
            scale.field,
            Op::Range,
            vec![1, i64::from(scale.max)],
            vec![],
        );
        assert_eq!(
            index.query(&[range], 10, 100).unwrap().is_ok(),
            scale.ordered
        );
    }
}

#[test]
fn a_code_above_the_maximum_is_not_stated_and_a_gap_code_falls_inside_its_range() {
    use dyapp_profile::{CareerAmbition, EducationLevel, Gender};
    let index = index();
    let (newer, profile, record) = signed(
        &Identity::generate(),
        Profile {
            version: 1,
            gender: Gender::MAX + 1,
            career_ambition: CareerAmbition::MAX + 10,
            education_level: EducationLevel::MAX + 10,
            ..Profile::default()
        },
    );
    index.put(&newer, &profile, &record, 0, true).unwrap();
    let (between, profile, record) = signed(
        &Identity::generate(),
        Profile {
            version: 1,
            career_ambition: 15,
            education_level: 45,
            ..Profile::default()
        },
    );
    index.put(&between, &profile, &record, 0, true).unwrap();
    let hits = |field, op, values: &[i64]| {
        let query = [cond(field, op, values.to_vec(), vec![])];
        let found = index.query(&query, 10, 100).unwrap().unwrap();
        found.hits.iter().map(|hit| hit.0).collect::<Vec<_>>()
    };
    // Stored, yet in no filter; asking for it is no error, so a newer client's query still runs.
    assert_eq!(
        hits(Gender::FIELD, Op::Eq, &[i64::from(Gender::MAX + 1)]),
        [] as [[u8; 32]; 0]
    );
    assert_eq!(
        hits(CareerAmbition::FIELD, Op::Range, &[20, 40]),
        [] as [[u8; 32]; 0]
    );
    assert_eq!(hits(27, Op::Eq, &[70]), [] as [[u8; 32]; 0]);
    assert_eq!(hits(CareerAmbition::FIELD, Op::Range, &[10, 20]), [between]);
    assert_eq!(hits(27, Op::Range, &[40, 50]), [between]);
}

#[test]
fn query_filters_height_with_inclusive_bounds_and_skips_unset() {
    let index = index();
    for height in [None, Some(170), Some(180)] {
        let (key, profile, record) = signed(
            &Identity::generate(),
            Profile {
                version: 1,
                height,
                ..Profile::default()
            },
        );
        index.put(&key, &profile, &record, 0, true).unwrap();
    }
    let height = |from, to| {
        [cond(
            dyapp_profile::HEIGHT_FIELD,
            Op::Range,
            vec![from, to],
            vec![],
        )]
    };
    let count = |from, to| {
        let found = index.query(&height(from, to), 10, 100).unwrap().unwrap();
        assert_eq!(found.applied, [dyapp_profile::HEIGHT_FIELD]);
        found.hits.len()
    };
    assert_eq!(count(170, 180), 2);
    assert_eq!(count(171, 272), 1);
    assert_eq!(count(50, 170), 1);
    assert_eq!(count(171, 179), 0);
    assert!(index.query(&height(180, 170), 10, 100).unwrap().is_err());
}

#[test]
fn query_filters_photos() {
    let (index, _) = population();
    let found = index
        .query(&[cond(21, Op::Range, vec![1, 16], vec![])], 200, 10_000)
        .unwrap()
        .unwrap();
    assert_eq!(found.applied, vec![21]);
    assert_eq!(found.hits.len(), 150);
}

#[test]
fn query_filters_activity_by_day() {
    let index = index();
    let day = 86_400;
    let mut keys = Vec::new();
    for now in [10 * day, 12 * day - 1, 12 * day] {
        let (key, p, r) = signed(&Identity::generate(), Profile::default());
        index.put(&key, &p, &r, now, true).unwrap();
        keys.push(key);
    }
    index.heartbeat(&keys[1], 0, 13 * day).unwrap();
    let since = |first: i64| {
        let q = [cond(1000, Op::Range, vec![first, i64::MAX], vec![])];
        let found = index.query(&q, 200, 10_000).unwrap().unwrap();
        assert_eq!(found.applied, vec![1000]);
        let mut hits: Vec<_> = found.hits.iter().map(|h| h.0).collect();
        hits.sort_unstable();
        hits
    };
    let mut want = vec![keys[1], keys[2]];
    want.sort_unstable();
    assert_eq!(since(12), want);
    assert_eq!(since(13), [keys[1]]);
    assert_eq!(since(10).len(), 3);
}

#[test]
fn query_filters_gender() {
    let (index, p03) = population();
    let q = [
        cond(dyapp_profile::Gender::FIELD, Op::Eq, vec![1], vec![]),
        cond(19, Op::Eq, vec![], vec!["chess"]),
    ];
    let found = index.query(&q, 200, 10_000).unwrap().unwrap();
    assert_eq!(found.applied, vec![dyapp_profile::Gender::FIELD, 19]);
    // Chess and woman is i % 6 == 0: the 100 chess players in SIX[0].
    assert_eq!(found.hits.len(), 100);
    assert!(found.hits.iter().all(|h| p03.contains(&h.0)));
}

#[test]
fn query_filters_place_in_and_budget() {
    let (index, p03) = population();
    let q = [
        cond(4, Op::Eq, vec![], vec!["DE"]),
        cond(20, Op::In, vec![], vec![SIX[1], SIX[2]]),
        cond(BIRTH_AGE, Op::Range, vec![19_861_025, 19_961_025], vec![]),
        cond(19, Op::In, vec![], vec!["chess", "go"]),
        cond(99, Op::Eq, vec![1], vec![]),
    ];
    let found = index.query(&q, 200, 10_000).unwrap().unwrap();
    assert_eq!(found.applied, vec![4, 20, BIRTH_AGE, 19]);
    assert!(!found.partial);
    // Places p1/p2 are i % 6 in {1, 2}, so i % 3 != 0: nobody there plays chess.
    assert_eq!(found.hits, [] as [([u8; 32], u64); 0]);
    let q = [
        cond(4, Op::Eq, vec![], vec!["DE"]),
        cond(20, Op::In, vec![], vec![SIX[0], SIX[3]]),
        cond(19, Op::Eq, vec![], vec!["chess"]),
    ];
    let found = index.query(&q, 200, 10_000).unwrap().unwrap();
    // p0/p3 are the 200 rows with i % 3 == 0, all with chess.
    assert_eq!(found.hits.len(), 200);
    let keys: std::collections::HashSet<_> = found.hits.iter().map(|h| h.0).collect();
    assert_eq!(keys, p03);
    let small = index.query(&q, 50, 30).unwrap().unwrap();
    assert!(small.partial && small.hits.len() <= 30);
    let a = index.query(&q, 10, 10_000).unwrap().unwrap().hits;
    assert_eq!(a.len(), 10);
    // A new random start each time.
    assert!((0..5).any(|_| index.query(&q, 10, 10_000).unwrap().unwrap().hits != a));
}

#[test]
fn mutual_conditions_keep_only_who_seeks_the_gender_and_the_age() {
    let index = index();
    let put = |genders: Vec<u32>, from, to, not_looking| {
        let (key, p, r) = signed(
            &Identity::generate(),
            Profile {
                version: 1,
                seeking_genders: genders,
                seeking_age_from: from,
                seeking_age_to: to,
                not_looking,
                ..Profile::default()
            },
        );
        index.put(&key, &p, &r, 0, true).unwrap();
        key
    };
    let bounded = put(vec![1, 2], Some(25), Some(35), false);
    let open_above = put(vec![1], Some(25), None, false);
    let open_below = put(vec![1], None, Some(35), false);
    let no_age = put(vec![1], None, None, false);
    let no_gender = put(vec![], Some(25), Some(35), false);
    let not_looking = put(vec![], None, None, true);
    let found = |gender: i64, age: i64| {
        let q = [
            cond(53, Op::Eq, vec![gender], vec![]),
            cond(54, Op::Eq, vec![age], vec![]),
        ];
        let found = index.query(&q, 50, 10_000).unwrap().unwrap();
        assert_eq!(found.applied, [53, 54]);
        let mut keys: Vec<_> = found.hits.into_iter().map(|h| h.0).collect();
        keys.sort_unstable();
        keys
    };
    let sorted = |mut keys: Vec<[u8; 32]>| {
        keys.sort_unstable();
        keys
    };
    assert_eq!(found(1, 30), sorted(vec![bounded, open_above, open_below]));
    assert_eq!(found(2, 30), [bounded]);
    assert_eq!(found(1, 24), [open_below]);
    assert_eq!(found(1, 36), [open_above]);
    assert_eq!(found(3, 30), Vec::<[u8; 32]>::new());
    let all = index.query(&[], 50, 10_000).unwrap().unwrap().hits;
    assert_eq!(all.len(), 6);
    for key in [no_age, no_gender, not_looking] {
        assert!(all.iter().any(|h| h.0 == key));
    }
}

#[test]
fn a_republish_replaces_who_a_profile_seeks() {
    let index = index();
    let id = Identity::generate();
    let seeking = Profile {
        version: 1,
        seeking_genders: vec![1],
        seeking_age_from: Some(25),
        ..Profile::default()
    };
    let (key, p, r) = signed(&id, seeking);
    index.put(&key, &p, &r, 0, true).unwrap();
    let q = [
        cond(53, Op::Eq, vec![1], vec![]),
        cond(54, Op::Eq, vec![30], vec![]),
    ];
    let hits = || index.query(&q, 50, 10_000).unwrap().unwrap().hits.len();
    assert_eq!(hits(), 1);
    let (key, p, r) = signed(
        &id,
        Profile {
            version: 2,
            not_looking: true,
            ..Profile::default()
        },
    );
    index.put(&key, &p, &r, 0, false).unwrap();
    assert_eq!(hits(), 0);
    let gender_only = [cond(53, Op::Eq, vec![1], vec![])];
    let found = index.query(&gender_only, 50, 10_000).unwrap().unwrap();
    assert_eq!(found.hits, [] as [([u8; 32], u64); 0]);
}

#[test]
fn invalid_conditions() {
    let (index, _) = population();
    let bad = |q: Vec<Condition>| index.query(&q, 50, 10_000).unwrap().is_err();
    assert!(bad(vec![cond(
        BIRTH_AGE,
        Op::Range,
        vec![],
        vec!["thirty"]
    )]));
    assert!(bad(vec![cond(4, Op::Eq, vec![1], vec![])]));
    for malformed in [
        cond(54, Op::In, vec![20], vec![]),
        cond(54, Op::Eq, vec![20, 30], vec![]),
        cond(54, Op::Eq, vec![], vec![]),
        cond(54, Op::Eq, vec![20], vec!["twenty"]),
    ] {
        assert!(bad(vec![malformed]));
    }
    assert!(bad(vec![cond(BIRTH_AGE, Op::Range, vec![40, 30], vec![])]));
    assert!(bad(vec![
        cond(BIRTH_AGE, Op::Range, vec![1, 2], vec![]),
        cond(BIRTH_AGE, Op::Range, vec![3, 4], vec![])
    ]));
    assert!(!bad(vec![cond(4, Op::In, vec![], vec!["a"; 16])]));
    assert!(bad(vec![cond(4, Op::In, vec![], vec!["a"; 17])]));
    assert!(bad((0..33)
        .map(|f| cond(100 + f, Op::Eq, vec![1], vec![]))
        .collect()));
    let skipped = index
        .query(&[cond(4, Op::Range, vec![], vec!["a", "b"])], 50, 10_000)
        .unwrap()
        .unwrap();
    assert_eq!(skipped.applied, [] as [u32; 0]);
    let unknown_op = Condition {
        field: BIRTH_AGE,
        op: 42,
        ints: vec![1],
        strings: vec![],
    };
    assert_eq!(
        index
            .query(&[unknown_op], 50, 10_000)
            .unwrap()
            .unwrap()
            .applied,
        [] as [u32; 0]
    );
}
