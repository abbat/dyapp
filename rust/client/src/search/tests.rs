#[path = "tests/withdrawal_tests.rs"]
mod withdrawal_tests;

use super::*;
use dyapp_identity::Identity;
use dyapp_p2p_net::proto::{search_request::Request, Op, SearchHit, SearchResponse, Status};
use dyapp_p2p_net::{build_configured_swarm, BehaviourEvent, Protocols, SwarmOptions};
use dyapp_profile::Profile;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::request_response::{Event, Message};
use libp2p::swarm::SwarmEvent;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn record(identity: &Identity, version: u64, age: u32) -> SignedRecord {
    Profile {
        version,
        birth_year: Some(2026 - age),
        birth_month: Some(1),
        birth_day: Some(1),
        ..Profile::default()
    }
    .sign(identity)
}

fn key(identity: &Identity) -> Vec<u8> {
    dyapp_identity::key_hash(&identity.public_key()).to_vec()
}

fn hit(key: Vec<u8>, version: u64) -> SearchHit {
    SearchHit { key, version }
}

fn ok(hits: Vec<SearchHit>, records: Vec<SignedRecord>, partial: bool) -> SearchResponse {
    SearchResponse {
        status: Status::Ok.into(),
        hits,
        records,
        partial,
        ..SearchResponse::default()
    }
}

type Requests = Arc<Mutex<Vec<Request>>>;

// A search node that answers each request with `answer` and keeps what it was asked.
async fn fake(
    mut answer: impl FnMut(&Request) -> SearchResponse + Send + 'static,
) -> (Node, Requests) {
    let options = SwarmOptions {
        search: true,
        ..SwarmOptions::default()
    };
    let mut server =
        Box::new(build_configured_swarm(Keypair::generate_ed25519(), options).unwrap());
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            break address;
        }
    };
    let requests = Requests::default();
    let seen = Arc::clone(&requests);
    tokio::spawn(async move {
        loop {
            if let SwarmEvent::Behaviour(BehaviourEvent::Search(Event::Message {
                message:
                    Message::Request {
                        request, channel, ..
                    },
                ..
            })) = server.select_next_some().await
            {
                let request = request.request.unwrap();
                let response = answer(&request);
                seen.lock().unwrap().push(request);
                let _ = server
                    .behaviour_mut()
                    .search
                    .send_response(channel, response);
            }
        }
    });
    let node = Node::connect(address, Arc::new(Protocols::public()))
        .await
        .unwrap();
    (node, requests)
}

fn gets(requests: &Requests) -> Vec<Vec<Vec<u8>>> {
    let requests = requests.lock().unwrap();
    let gets = requests.iter().filter_map(|request| match request {
        Request::Get(get) => Some(get.keys.clone()),
        _ => None,
    });
    gets.collect()
}

#[tokio::test]
async fn age_filter_marks_skipped_node_partial_and_rechecks_its_records() {
    let [a, b, c] = [(); 3].map(|()| Identity::try_generate().unwrap());
    let dated = |identity: &Identity| {
        Profile {
            version: 1,
            birth_year: Some(2000),
            ..Profile::default()
        }
        .sign(identity)
    };
    let yearless = Profile {
        version: 1,
        birth_month: Some(2),
        birth_day: Some(29),
        ..Profile::default()
    }
    .sign(&b);
    let mut nodes = Vec::new();
    for (hits, records, applied) in [
        (
            vec![hit(key(&a), 1), hit(key(&b), 1)],
            vec![dated(&a), yearless],
            vec![search_results::BIRTH_AGE],
        ),
        (vec![hit(key(&c), 1)], vec![dated(&c)], vec![]),
    ] {
        let mut query = ok(hits, Vec::new(), false);
        query.applied = applied;
        let mut answers = vec![ok(Vec::new(), records, false), query];
        nodes.push(fake(move |_| answers.pop().unwrap()).await.0);
    }
    let conditions = [Condition {
        field: search_results::BIRTH_AGE,
        op: Op::Range.into(),
        ints: vec![19_991_231, 20_001_231],
        strings: Vec::new(),
    }];
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        search(&mut nodes, &conditions, 0, 0, &Profiles::default(), &[]),
    )
    .await
    .unwrap();
    assert!(result.partial);
    let mut peers: Vec<_> = result.profiles.iter().map(|p| p.peer_id.clone()).collect();
    peers.sort();
    let mut expected = vec![a.peer_id(), c.peer_id()];
    expected.sort();
    assert_eq!(peers, expected);
}

#[test]
fn cache_keys_deduplicates_with_the_highest_version_before_truncating() {
    let [a, b, c] = [(); 3].map(|()| Identity::try_generate().unwrap());
    let mut cache = Profiles::default();
    cache
        .accept(dyapp_profile::verify(&record(&a, 2, 30)).unwrap())
        .unwrap();
    let mut cached = Vec::new();
    let keys = cache_keys(
        vec![
            hit(key(&a), 1),
            hit(key(&b), 1),
            hit(key(&a), 3),
            hit(key(&c), 1),
        ],
        2,
        &cache,
        &[],
        &mut cached,
    );
    assert_eq!(keys, [key(&a), key(&b)]);
    assert_eq!(cached.len(), 0);
}

#[test]
fn offers_only_fields_every_node_lists_and_the_client_checks() {
    let info = |fields: &[u32]| proto::NodeInfo {
        search_fields: fields.to_vec(),
        ..proto::NodeInfo::default()
    };
    let (age, active) = (search_results::BIRTH_AGE, search_results::ACTIVE_DAY);
    // 5 is no searchable profile field and 999 is unknown: neither can be checked.
    let full = info(&[27, 4, 19, active, age, 5, 999, 4]);
    let narrow = info(&[4, 19, age, 5, 999]);
    assert_eq!(
        super::offered_fields(std::slice::from_ref(&full)),
        [4, 19, 27, active, age]
    );
    assert_eq!(super::offered_fields(&[full, narrow]), [4, 19, age]);
    assert_eq!(super::offered_fields(&[info(&[4]), info(&[])]), [0u32; 0]);
    assert_eq!(super::offered_fields(&[]), [] as [u32; 0]);
}

#[tokio::test]
async fn drops_nodes_that_skip_the_activity_condition() {
    let [a, b] = [(); 2].map(|()| Identity::try_generate().unwrap());
    let mut nodes = Vec::new();
    for (identity, applied) in [(&a, vec![search_results::ACTIVE_DAY]), (&b, vec![])] {
        let mut query = ok(vec![hit(key(identity), 1)], Vec::new(), false);
        query.applied = applied;
        let mut answers = vec![ok(Vec::new(), vec![record(identity, 1, 30)], false), query];
        nodes.push(fake(move |_| answers.pop().unwrap()).await.0);
    }
    let conditions = [Condition {
        field: search_results::ACTIVE_DAY,
        op: Op::Range.into(),
        ints: vec![2, i64::MAX],
        strings: Vec::new(),
    }];
    let cache = Profiles::default();
    let search = search(&mut nodes, &conditions, 0, 0, &cache, &[]);
    let found = tokio::time::timeout(Duration::from_secs(10), search)
        .await
        .unwrap();
    assert!(found.partial);
    let peers: Vec<_> = found.profiles.iter().map(|p| p.peer_id.clone()).collect();
    assert_eq!(peers, [a.peer_id()]);
}

async fn second_search_node(
    uncached: &Identity,
    cached: &Identity,
    unprompted: &Identity,
) -> (Node, Requests) {
    // The reply exceeds the default limit; only 49 uncached keys fit after the cached hit.
    let mut hits: Vec<_> = (0..51u8).map(|n| hit(vec![n; 32], 1)).collect();
    // A whole reply that misses keys is not asked again.
    hits[0] = hit(key(unprompted), 1);
    hits.insert(0, hit(key(cached), 4));
    let mut responses = vec![
        ok(hits, Vec::new(), false),
        ok(
            Vec::new(),
            vec![
                record(uncached, 1, 30),
                record(cached, 4, 30),
                record(unprompted, 1, 30),
            ],
            false,
        ),
        ok(Vec::new(), Vec::new(), false),
    ]
    .into_iter();
    fake(move |_| responses.next().unwrap()).await
}

async fn assert_merged_search(
    nodes: &mut [Node],
    conditions: &[Condition],
    cache: &Profiles,
    blocked: &[String],
    one_asked: &Requests,
    two_asked: &Requests,
    identities: &[Identity; 8],
) {
    let [newer, _, _, _, cached, older, missing, unprompted] = identities;
    let search = search(nodes, conditions, 0, 0, cache, blocked);
    let found = tokio::time::timeout(Duration::from_secs(10), search)
        .await
        .unwrap();
    assert!(found.partial); // One node failed and two get replies omitted keys.
    let mut found: Vec<_> = found
        .profiles
        .iter()
        .map(|p| (p.peer_id.clone(), p.profile.version))
        .collect();
    found.sort();
    let mut expected = vec![
        (newer.peer_id(), 2),
        (older.peer_id(), 5),
        (unprompted.peer_id(), 1),
    ];
    expected.sort();
    assert_eq!(found, expected);
    let one_gets = gets(one_asked);
    assert_eq!(one_gets.len(), 3);
    assert_eq!(one_gets[0], [key(newer), key(older), key(missing)]);
    assert_eq!(one_gets[1].len(), 2);
    assert_eq!(one_gets[2], [key(missing)]);
    let two_gets = gets(two_asked);
    assert_eq!(two_gets.iter().map(Vec::len).collect::<Vec<_>>(), [49]);
    assert!(!two_gets.concat().contains(&key(cached)));
}

#[tokio::test]
#[expect(
    clippy::many_single_char_names,
    reason = "Alphabetic identities track the distinct cache, age-filter and retry cases in this fixture"
)]
async fn merges_nodes_skips_cached_hits_and_resends_cut_gets() {
    let [a, b, c, d, e, f, g, h] = [(); 8].map(|()| Identity::try_generate().unwrap());
    let mut cache = Profiles::default();
    let held = [(&a, 1, 30), (&b, 1, 30), (&e, 5, 60), (&f, 5, 31)];
    for (identity, version, age) in held {
        let held = record(identity, version, age);
        cache.accept(dyapp_profile::verify(&held).unwrap()).unwrap();
    }
    // a fits and is newer than cached, b is cached, c misses the age, f is served older than
    // cached, g never comes.
    let hits = vec![
        hit(key(&a), 2),
        hit(key(&b), 1),
        hit(key(&c), 3),
        hit(key(&f), 6),
        hit(key(&g), 1),
    ];
    let mut first = vec![
        ok(hits, Vec::new(), false),
        ok(Vec::new(), vec![record(&a, 2, 30)], true),
        ok(Vec::new(), vec![record(&c, 3, 50), record(&f, 4, 30)], true),
        ok(Vec::new(), Vec::new(), true),
    ]
    .into_iter();
    let (one, one_asked) = fake(move |_| first.next().unwrap()).await;
    let (two, two_asked) = second_search_node(&d, &e, &h).await;
    let (down, _) = fake(|_| SearchResponse {
        status: Status::Denied.into(),
        ..SearchResponse::default()
    })
    .await;
    let conditions = [Condition {
        field: search_results::BIRTH_AGE,
        op: Op::Range.into(),
        ints: vec![19_901_025, 20_011_025],
        strings: Vec::new(),
    }];
    let mut nodes = vec![one, two, down].into_boxed_slice();
    // Blocked: b is cached and c would be fetched.
    let blocked = [b.peer_id(), c.peer_id()];
    let identities = [a, b, c, d, e, f, g, h];
    assert_merged_search(
        &mut nodes,
        &conditions,
        &cache,
        &blocked,
        &one_asked,
        &two_asked,
        &identities,
    )
    .await;
}

#[tokio::test]
async fn classifies_failures_and_drops_forged_records() {
    let [a, b] = [(); 2].map(|()| Identity::try_generate().unwrap());
    let mut forged = record(&b, 1, 30);
    forged.signature[0] ^= 1;
    let mut answers = vec![
        ok(Vec::new(), vec![record(&a, 1, 30), forged], false),
        ok(vec![hit(key(&a), 1), hit(key(&b), 1)], Vec::new(), false),
    ];
    let (liar, _) = fake(move |_| answers.pop().unwrap()).await;
    let (busy, _) = fake(|_| status(Status::RateLimited, 30)).await;
    let (hard, _) = fake(|_| SearchResponse {
        status: Status::Puzzle.into(),
        puzzle_challenge: vec![3; 16],
        puzzle_bits: crate::MAX_PUZZLE_BITS + 1,
        ..SearchResponse::default()
    })
    .await;
    let (broken, _) = fake(|_| status(Status::Invalid, 0)).await;
    let (refusing, _) = fake(|_| status(Status::Refused, 0)).await;
    let mut nodes = vec![liar, busy, hard, broken, refusing].into_boxed_slice();
    let cache = Profiles::default();
    let search = search(&mut nodes, &[], 0, 0, &cache, &[]);
    let found = tokio::time::timeout(Duration::from_secs(10), search)
        .await
        .unwrap();
    assert!(found.partial);
    let peers: Vec<_> = found.profiles.iter().map(|p| p.peer_id.clone()).collect();
    assert_eq!(peers, [a.peer_id()]);
    let mut failures = found.failures;
    failures.sort_by_key(|failure| match failure {
        SearchFailure::Invalid => 0,
        SearchFailure::RateLimited(_) => 1,
        SearchFailure::Puzzle => 2,
        SearchFailure::Unavailable => 3,
        SearchFailure::NoNodes | SearchFailure::Timeout => 4,
    });
    assert_eq!(
        failures,
        [
            SearchFailure::Invalid,
            SearchFailure::Invalid,
            SearchFailure::RateLimited(Duration::from_secs(30)),
            SearchFailure::Puzzle,
            SearchFailure::Unavailable,
        ]
    );
    let none = super::search(&mut [], &[], 0, 0, &cache, &[]).await;
    assert!(none.partial && none.profiles.is_empty());
    assert_eq!(none.failures, [SearchFailure::NoNodes]);
}

#[test]
fn maps_node_errors_to_search_failures() {
    for (error, failure) in [
        (Error::Timeout, SearchFailure::Timeout),
        (Error::Connect(String::new()), SearchFailure::Unavailable),
        (Error::Request(String::new()), SearchFailure::Unavailable),
        (Error::Status(Status::Puzzle), SearchFailure::Puzzle),
        (Error::Reply("bad hit"), SearchFailure::Invalid),
    ] {
        assert_eq!(SearchFailure::from(&error), failure);
    }
}

async fn within(publish: impl std::future::Future<Output = Result<bool, store::Error>>) -> bool {
    tokio::time::timeout(Duration::from_secs(10), publish)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn persistent_scores_skip_an_invalid_node_on_the_next_search() {
    let identity = Identity::try_generate().unwrap();
    let (invalid, invalid_asked) = fake(move |_| {
        ok(
            vec![hit(key(&identity), 1), hit(key(&identity), 1)],
            Vec::new(),
            false,
        )
    })
    .await;
    let (valid, valid_asked) = fake(|_| ok(Vec::new(), Vec::new(), false)).await;
    let mut nodes = vec![invalid, valid].into_boxed_slice();
    let cache = Profiles::default();
    let mut scores = Scores::default();
    let first = search_with_scores(&mut nodes, &mut scores, &[], 1, 0, &cache, &[]).await;
    assert_eq!(first.failures, [SearchFailure::Invalid]);
    let invalid_first = invalid_asked.lock().unwrap().len();
    assert!(invalid_first >= 1);
    assert_eq!(valid_asked.lock().unwrap().len(), 1);
    let second = search_with_scores(&mut nodes, &mut scores, &[], 1, 0, &cache, &[]).await;
    assert_eq!(second.failures, []);
    assert_eq!(invalid_asked.lock().unwrap().len(), invalid_first);
    assert_eq!(valid_asked.lock().unwrap().len(), 2);
}
fn status(status: Status, retry_after: u64) -> SearchResponse {
    SearchResponse {
        status: status.into(),
        retry_after,
        ..SearchResponse::default()
    }
}

#[tokio::test]
async fn publishes_the_held_profile_once_due_and_waits_out_rate_limits() {
    let alice = Identity::try_generate().unwrap();
    let store = Store::open(&crate::store::tests::temp_db_path(), &alice).unwrap();
    let first = record(&alice, 1, 30);
    let busy = [
        (Status::RateLimited, 60),
        (Status::Denied, 0),
        (Status::Ok, 0),
    ];
    let mut busy = busy.map(|(s, after)| status(s, after)).into_iter();
    let (busy, busy_asked) = fake(move |_| busy.next().unwrap()).await;
    let slow = [
        (Status::RateLimited, 120),
        (Status::Stale, 0),
        (Status::Denied, 0),
    ];
    let mut slow = slow.map(|(s, after)| status(s, after)).into_iter();
    let (slow, _) = fake(move |_| slow.next().unwrap()).await;
    let mut nodes = vec![busy, slow].into_boxed_slice();
    // Nothing held: no node is asked.
    assert!(!within(publish_due(&store, &mut nodes, 1000)).await);
    store.hold_search_profile(&first).unwrap();
    // Both nodes busy: held for the longer wait.
    assert!(!within(publish_due(&store, &mut nodes, 1000)).await);
    assert_eq!(store.due_search_profile(1119).unwrap(), None);
    assert_eq!(store.due_search_profile(1120).unwrap(), Some(first.clone()));
    // One fails and the other has it already: the window starts.
    assert!(within(publish_due(&store, &mut nodes, 1120)).await);
    assert_eq!(store.search_profile_minutes_left(1120).unwrap(), None);
    let second = record(&alice, 2, 30);
    store.hold_search_profile(&second).unwrap();
    assert_eq!(store.due_search_profile(1719).unwrap(), None);
    // One takes it.
    assert!(within(publish_due(&store, &mut nodes, 1720)).await);
    assert_eq!(store.due_search_profile(9999).unwrap(), None);
    let asked = busy_asked.lock().unwrap();
    let sent: Vec<_> = asked
        .iter()
        .map(|r| match r {
            Request::Publish(sent) => sent.clone(),
            _ => panic!("not a publish"),
        })
        .collect();
    assert_eq!(sent, [first.clone(), first, second]);
}

#[tokio::test]
async fn beats_once_a_day_with_a_proof_for_the_day() {
    let alice = Identity::try_generate().unwrap();
    let store = Store::open(&crate::store::tests::temp_db_path(), &alice).unwrap();
    let answers = [Status::Denied, Status::RateLimited, Status::Ok];
    let mut answers = answers.map(|s| status(s, 60)).into_iter();
    let (node, asked) = fake(move |_| answers.next().unwrap()).await;
    let mut nodes = [node];
    let (day, now) = (20_000, 20_000 * 86_400 + 5);
    assert!(!within(beat_due(&store, &alice, 3, 2, &mut nodes, now)).await);
    // Two already today: done for the day.
    assert!(within(beat_due(&store, &alice, 3, 2, &mut nodes, now)).await);
    assert!(!within(beat_due(&store, &alice, 3, 2, &mut nodes, now + 86_394)).await);
    // Due at a random second of the next day, so by its last second.
    let next = u64::from(day + 2) * 86_400 - 1;
    assert!(within(beat_due(&store, &alice, 3, 2, &mut nodes, next)).await);
    {
        let requests = asked.lock().unwrap();
        assert_eq!(requests.len(), 3);
        let mut heartbeat = None;
        if let Request::Heartbeat(record) = &requests[2] {
            heartbeat = Some(record);
        }
        let record = heartbeat.expect("not a heartbeat");
        let beat: proto::Heartbeat = prost::Message::decode(record.payload.as_slice()).unwrap();
        assert_eq!((beat.time, beat.version, beat.pow_day), (next, 3, day + 1));
        let key = alice.public_key();
        assert!(dyapp_profile::heartbeat_pow_bits(&key, day + 1, beat.pow_nonce) >= 2);
        assert_eq!(beat.pow_bits, 2);
        assert_eq!(record.public_key, key.to_vec());
    }
    store.set_profile_hidden(true).unwrap();
    assert!(!within(beat_due(&store, &alice, 3, 2, &mut nodes, next + 86_400)).await);
    assert_eq!(asked.lock().unwrap().len(), 3);
}

#[test]
fn employer_filter_ignores_case_and_spacing_and_keeps_profiles_without_a_company() {
    let found = |company: &str| {
        let profile = Profile {
            version: 1,
            company: company.into(),
            ..Profile::default()
        };
        dyapp_profile::verify(&profile.sign(&Identity::try_generate().unwrap())).unwrap()
    };
    let all = vec![found("Acme  Corp"), found(""), found("Other")];
    let kept = |filter: Employer| {
        let mut profiles = all.clone();
        filter.retain(&mut profiles);
        profiles
            .iter()
            .map(|p| p.profile.company.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(kept(Employer::Any), ["Acme  Corp", "", "Other"]);
    assert_eq!(kept(Employer::Same(" acme corp ".into())), ["Acme  Corp"]);
    assert_eq!(
        kept(Employer::Same("  ".into())),
        [] as [std::string::String; 0]
    );
    assert_eq!(
        kept(Employer::Exclude(vec!["ACME CORP".into(), String::new()])),
        ["", "Other"]
    );
}
