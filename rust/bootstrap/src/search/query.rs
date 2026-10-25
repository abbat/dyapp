use super::tests::{index, signed};
use super::*;
use dyapp_identity::Identity;
use dyapp_p2p_net::proto::{Condition, Op};

fn cond(field: u32, op: Op, ints: Vec<i64>, strings: Vec<&str>) -> Condition {
    Condition {
        field,
        op: op.into(),
        ints,
        strings: strings.into_iter().map(String::from).collect(),
    }
}

/// 600 profiles: places p0..p5 in "DE", ages 20..79, interest "chess" on every 3rd, gender
/// "f" on every 2nd, education level `i % 7`.
/// GeoNames IDs of Berlin, Hamburg, Munich, Cologne, Frankfurt and Stuttgart.
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
            age: u32::try_from(20 + i % 60).unwrap(),
            country: "DE".into(),
            place: SIX[(i % 6) as usize].into(),
            gender: if i % 2 == 0 { "f" } else { "m" }.into(),
            education_level: u32::try_from(i % 7).unwrap(),
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
fn query_filters_least_education() {
    let (index, _) = population();
    let found = index
        .query(&[cond(27, Op::Range, vec![5, 6], vec![])], 200, 10_000)
        .unwrap()
        .unwrap();
    assert_eq!(found.applied, vec![27]);
    // i % 7 in {5, 6}: 85 + 85 of the 600 (i = 5, 12, ..., 593 and 6, ..., 594).
    assert_eq!(found.hits.len(), 170);
}

#[test]
fn query_filters_gender() {
    let (index, p03) = population();
    let q = [
        cond(25, Op::Eq, vec![], vec!["f"]),
        cond(19, Op::Eq, vec![], vec!["chess"]),
    ];
    let found = index.query(&q, 200, 10_000).unwrap().unwrap();
    assert_eq!(found.applied, vec![25, 19]);
    // Chess and "f" is i % 6 == 0: the 100 chess players in SIX[0].
    assert_eq!(found.hits.len(), 100);
    assert!(found.hits.iter().all(|h| p03.contains(&h.0)));
}

#[test]
fn query_filters_place_in_and_budget() {
    let (index, p03) = population();
    let q = [
        cond(4, Op::Eq, vec![], vec!["DE"]),
        cond(20, Op::In, vec![], vec![SIX[1], SIX[2]]),
        cond(3, Op::Range, vec![30, 39], vec![]),
        cond(19, Op::In, vec![], vec!["chess", "go"]),
        cond(99, Op::Eq, vec![1], vec![]),
    ];
    let found = index.query(&q, 200, 10_000).unwrap().unwrap();
    assert_eq!(found.applied, vec![4, 20, 3, 19]);
    assert!(!found.partial);
    // Places p1/p2 are i % 6 in {1, 2}, so i % 3 != 0: nobody there plays chess.
    assert!(found.hits.is_empty());
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
fn invalid_conditions() {
    let (index, _) = population();
    let bad = |q: Vec<Condition>| index.query(&q, 50, 10_000).unwrap().is_err();
    assert!(bad(vec![cond(3, Op::Eq, vec![], vec!["thirty"])]));
    assert!(bad(vec![cond(4, Op::Eq, vec![1], vec![])]));
    assert!(bad(vec![cond(3, Op::Range, vec![40, 30], vec![])]));
    assert!(bad(vec![
        cond(3, Op::Eq, vec![1], vec![]),
        cond(3, Op::Eq, vec![2], vec![])
    ]));
    assert!(bad(vec![cond(4, Op::In, vec![], vec!["a"; 9])]));
    assert!(bad((0..17)
        .map(|f| cond(100 + f, Op::Eq, vec![1], vec![]))
        .collect()));
    let skipped = index
        .query(&[cond(4, Op::Range, vec![], vec!["a", "b"])], 50, 10_000)
        .unwrap()
        .unwrap();
    assert!(skipped.applied.is_empty());
    let unknown_op = Condition {
        field: 3,
        op: 42,
        ints: vec![1],
        strings: vec![],
    };
    assert!(index
        .query(&[unknown_op], 50, 10_000)
        .unwrap()
        .unwrap()
        .applied
        .is_empty());
}
