mod slots;

use super::*;
use dyapp_identity::network::{self, Network, Settings};
use std::time::Duration;

fn directory(tag: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/native-search-tests")
        .join(tag)
        .join(IdentityKey::generate().peer_id());
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn search_needs_an_answering_active_network() {
    let dir = directory("search");
    let key = IdentityKey::generate();
    let searcher = ProfileSearcher::new(dir.to_str().unwrap().to_owned()).unwrap();
    let network = Network {
        seeds: Vec::new(),
        name: String::new(),
    };
    network::save(&dir, Settings::Draft, &network).unwrap();
    let search = || searcher.search(key.clone(), Vec::new(), SearchEmployer::Any, 0);
    assert_eq!(search().unwrap_err().to_string(), "notConnected");
    network::save(&dir, Settings::Active, &network).unwrap();
    // A retry is a new call on the same searcher.
    assert_eq!(search().unwrap_err().to_string(), "notConnected");
}

#[test]
fn search_dials_from_a_swift_sized_stack() {
    let dir = directory("stack");
    let (listener, _udp) = crate::network::tests::reserved_endpoint();
    let closed = listener.local_addr().unwrap().port();
    let rejected = crate::network::tests::reject_handshake(listener.try_clone().unwrap());
    let network = Network {
        seeds: vec![format!("127.0.0.1:{closed}")],
        name: String::new(),
    };
    network::save(&dir, Settings::Active, &network).unwrap();
    let searcher = ProfileSearcher::new(dir.to_str().unwrap().to_owned()).unwrap();
    // Without its own stack, building the swarm for the dial overflows this thread.
    let (finished, completed) = std::sync::mpsc::channel();
    let thread = std::thread::Builder::new()
        .stack_size(512 << 10)
        .spawn(move || {
            let key = IdentityKey::generate();
            let result = searcher
                .search(key, Vec::new(), SearchEmployer::Any, 0)
                .map(|_| ());
            drop(searcher);
            finished.send(result).unwrap();
        })
        .unwrap();
    let outcome = completed.recv_timeout(Duration::from_secs(15)).unwrap();
    assert!(outcome.is_err());
    thread.join().unwrap();
    rejected.join().unwrap();
}

#[test]
fn a_sample_is_partial_after_a_short_answer_or_an_unreached_node() {
    assert!(!is_partial::<u8>(false, &[]));
    assert!(is_partial::<u8>(true, &[]));
    assert!(is_partial(false, &[1]));
}

#[test]
fn a_found_profile_lists_its_photo_hashes_in_order() {
    let photo = |full: Vec<Vec<u8>>, thumbnail: &[u8]| dyapp_profile::Photo {
        full,
        thumbnail: thumbnail.to_vec(),
    };
    let verified = dyapp_profile::VerifiedProfile {
        peer_id: "peer".into(),
        profile: dyapp_profile::Profile {
            version: 3,
            photos: vec![
                photo(vec![vec![1], vec![2]], &[10]),
                photo(Vec::new(), &[20]),
            ],
            ..dyapp_profile::Profile::default()
        },
    };
    let found = FoundProfile::from(&verified);
    assert_eq!(found.photos, vec![vec![10], vec![20]]);
    // The client publishes each full image as one blob; a missing one leaves an empty hash.
    assert_eq!(found.full_photos, vec![vec![1], Vec::new()]);
    assert_eq!((found.peer_id.as_str(), found.version), ("peer", 3));
}

#[test]
fn an_age_range_becomes_one_birth_range_condition() {
    let condition = search_age_condition(10, 30).unwrap();
    assert_eq!(condition.field, dyapp_p2p_net::search_results::BIRTH_AGE);
    assert_eq!(condition.op, SearchOp::Range);
    // The lower bound is raised to 18, so the range is shorter than 21 years.
    assert!(condition.ints[0] < condition.ints[1]);
    assert!(condition.ints[1] - condition.ints[0] < 14 * 10_000);
    assert_eq!(
        search_age_condition(30, 20).unwrap_err().to_string(),
        "invalid search age"
    );
}

#[test]
fn height_bounds_become_one_range_condition() {
    let ints = |from, to| {
        let condition = search_height_condition(from, to).unwrap().unwrap();
        assert_eq!(condition.op, SearchOp::Range);
        condition.ints
    };
    assert_eq!(ints(Some(170), None), vec![170, 272]);
    assert_eq!(ints(None, Some(180)), vec![50, 180]);
    assert_eq!(ints(Some(170), Some(180)), vec![170, 180]);
    assert!(search_height_condition(None, None).unwrap().is_none());
    assert_eq!(
        search_height_condition(Some(181), Some(180))
            .unwrap_err()
            .to_string(),
        "invalid search height"
    );
}

#[test]
fn a_mutual_search_needs_a_stated_gender_and_an_exact_adult_age() {
    let draft = |gender, birth_year, birth_month, birth_day| PublicProfileDraft {
        gender,
        birth_year,
        birth_month,
        birth_day,
        ..PublicProfileDraft::default()
    };
    let full = draft(2, Some(1990), Some(1), Some(1));
    let day = dyapp_profile::CivilDate::new(2026, 11, 11).unwrap();
    let conditions = mutual_at(&full, day);
    assert_eq!(
        conditions.iter().map(|c| c.field).collect::<Vec<_>>(),
        [53, 54]
    );
    assert!(conditions.iter().all(|c| c.op == SearchOp::Eq));
    assert_eq!(conditions[0].ints, [2]);
    assert_eq!(conditions[1].ints.len(), 1);
    assert_eq!(conditions[1].ints, [36]);
    for unavailable in [
        draft(0, Some(1990), Some(1), Some(1)),
        draft(2, None, None, None),
        draft(2, Some(2025), Some(1), Some(1)),
    ] {
        assert_eq!(mutual_at(&unavailable, day), [] as [SearchCondition; 0]);
    }
}

#[test]
fn conditions_and_failures_keep_their_meaning() {
    let condition = proto::Condition::from(SearchCondition {
        field: 1,
        op: SearchOp::In,
        ints: vec![1, 2],
        strings: Vec::new(),
    });
    assert_eq!(condition.op, i32::from(proto::Op::In));
    assert_eq!(condition.ints, vec![1, 2]);
    assert_eq!(
        ProfileSearchFailure::from(SearchFailure::RateLimited(Duration::from_secs(3))),
        ProfileSearchFailure::RateLimited {
            retry_after_ms: 3000
        }
    );
    assert_eq!(
        ProfileSearchFailure::from(SearchFailure::Invalid),
        ProfileSearchFailure::Invalid
    );
    assert_eq!(
        Employer::from(SearchEmployer::Exclude {
            companies: vec!["Acme".into()]
        }),
        Employer::Exclude(vec!["Acme".into()])
    );
}
