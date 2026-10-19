//! Search service tests.
use super::*;

/// A profile carrying a solved proof of work for `bits`.
fn proven(id: &Identity, mut profile: Profile, bits: u32) -> SignedRecord {
    profile.pow_nonce = dyapp_profile::solve(&id.public_key(), bits, 0, 100_000).unwrap();
    profile.pow_bits = bits;
    profile.sign(id)
}

/// Applies a gossip message, running its proof-of-work check inline as the node does off
/// the loop.
fn gossip(s: &Service, data: &[u8]) -> Verdict {
    match s.search_gossip(data) {
        Verdict::Pow(check) if s.pow.check(&check) => s.search_gossip(data),
        Verdict::Pow(_) => Verdict::Reject,
        verdict => verdict,
    }
}

/// Runs deferred work inline, as the node does in `spawn_blocking`.
pub(in crate::service) fn search_as(
    s: &Service,
    peer: &Peer,
    request: search_request::Request,
) -> SearchResponse {
    let index = s.search.clone().unwrap();
    let request = SearchRequest {
        request: Some(request),
    };
    match s.search(peer, request).unwrap() {
        Search::Reply(reply) => reply,
        Search::Query(q) => run_query(&index, &q, s.config.search.max_scan_rows).unwrap(),
        Search::Get(keys) => {
            let reply = run_get(&index, &keys).unwrap();
            let bytes = reply.records.iter().map(Message::encoded_len).sum();
            if s.charge_get(peer, bytes) {
                reply
            } else {
                search_reply(Status::RateLimited)
            }
        }
        Search::Pow(check, request) => {
            let ok = s.pow.check(&check);
            s.search_pow_done(peer, &request, ok).unwrap()
        }
    }
}

fn search(s: &Service, request: search_request::Request) -> SearchResponse {
    search_as(s, &peer("p"), request)
}

fn search_status(response: &SearchResponse) -> Status {
    Status::try_from(response.status).unwrap()
}

fn beat(identity: &Identity, version: u64, time: u64) -> SignedRecord {
    let beat = proto::Heartbeat {
        time,
        version,
        ..Default::default()
    };
    identity.sign(Domain::Heartbeat, beat.encode_to_vec())
}

#[test]
fn search_publish_query_get() {
    let s = search_service();
    let id = Identity::generate();
    let record = proven(
        &id,
        Profile {
            version: 1,
            age: 33,
            country: "DE".into(),
            ..Profile::default()
        },
        2,
    );
    let r = search(&s, search_request::Request::Publish(record.clone()));
    assert_eq!(search_status(&r), Status::Ok);
    let newer = proven(
        &id,
        Profile {
            version: 2,
            ..Profile::default()
        },
        2,
    );
    let r = search(&s, search_request::Request::Publish(newer));
    assert_eq!(search_status(&r), Status::RateLimited);
    assert!(r.retry_after > 0 && r.retry_after <= 600);
    let q = proto::SearchQuery {
        conditions: vec![proto::Condition {
            field: 3,
            op: proto::Op::Eq.into(),
            ints: vec![33],
            strings: vec![],
        }],
        limit: 0,
        puzzle: None,
    };
    let r = search(&s, search_request::Request::Query(q));
    assert_eq!(r.hits.len(), 1);
    assert_eq!(r.applied, vec![3]);
    assert_eq!(r.index_version, crate::search::INDEX_VERSION);
    let bad = proto::SearchQuery {
        conditions: vec![proto::Condition::default(); 17],
        limit: 0,
        puzzle: None,
    };
    let r = search(&s, search_request::Request::Query(bad));
    assert_eq!(search_status(&r), Status::Invalid);
    let get = |keys| search_request::Request::Get(proto::SearchGet { keys, puzzle: None });
    let key = r_key(&s, 33);
    let r = search(&s, get(vec![key.clone()]));
    assert_eq!(r.records, vec![record]);
    assert_eq!(
        search_status(&search(&s, get(vec![vec![0; 31]]))),
        Status::Invalid
    );
    assert_eq!(
        search_status(&search(&s, get(vec![key; 51]))),
        Status::Invalid
    );
    let now = chrono::Utc::now().timestamp().unsigned_abs();
    let r = search(&s, search_request::Request::Heartbeat(beat(&id, 1, now)));
    assert_eq!(search_status(&r), Status::Ok);
    let r = search(
        &s,
        search_request::Request::Heartbeat(beat(&id, 1, now - 3_600)),
    );
    assert_eq!(search_status(&r), Status::Invalid);
    let info = s.info();
    assert!(info.roles.contains(&proto::Role::Search.into()));
    assert_eq!(info.search_fields, SEARCH_FIELDS);
    assert_eq!(info.max_search_profile_bytes, 64 << 10);
    assert_eq!(info.min_profile_pow, 2);
    assert!(info
        .capabilities
        .contains(&proto::Capability::SearchPuzzle.into()));
    assert_eq!(
        service().info().capabilities,
        [i32::from(proto::Capability::ProfileHeartbeat)]
    );
    assert_eq!(service().info().min_profile_pow, 0);
    assert!(service().info().search_fields.is_empty());
}

/// The key of the one profile aged `age`.
fn r_key(s: &Service, age: i64) -> Vec<u8> {
    let q = proto::SearchQuery {
        conditions: vec![proto::Condition {
            field: 3,
            op: proto::Op::Eq.into(),
            ints: vec![age],
            strings: vec![],
        }],
        limit: 0,
        puzzle: None,
    };
    search_as(s, &peer("key"), search_request::Request::Query(q)).hits[0]
        .key
        .clone()
}

#[test]
fn search_puzzle_under_load() {
    let s = search_service();
    s.puzzle.set_bits(12);
    let me = peer("me");
    let query = |puzzle| {
        let request = SearchRequest {
            request: Some(search_request::Request::Query(proto::SearchQuery {
                puzzle,
                ..proto::SearchQuery::default()
            })),
        };
        s.search(&me, request).unwrap()
    };
    let Search::Reply(r) = query(None) else {
        panic!("no puzzle asked")
    };
    assert_eq!(search_status(&r), Status::Puzzle);
    assert_eq!(r.puzzle_bits, 12);
    let challenge: [u8; 16] = r.puzzle_challenge.try_into().unwrap();
    assert_eq!(s.info().puzzle_challenge, challenge.to_vec());
    let solution = proto::Puzzle {
        challenge: challenge.to_vec(),
        nonce: crate::puzzle::solve(&challenge, &me.id, 12),
    };
    assert!(matches!(query(Some(solution.clone())), Search::Query(_)));
    // The node consumes it on admission; a replay and another peer are asked again.
    s.puzzle.consume(&me.id, &solution);
    assert!(matches!(query(Some(solution.clone())), Search::Reply(_)));
    let get = SearchRequest {
        request: Some(search_request::Request::Get(proto::SearchGet {
            keys: vec![],
            puzzle: Some(solution),
        })),
    };
    let Search::Reply(r) = s.search(&peer("other"), get).unwrap() else {
        panic!()
    };
    assert_eq!(search_status(&r), Status::Puzzle);
    // Search nodes are never asked.
    let sync = peer("sync");
    s.search_peer(&sync.id, true);
    let request = SearchRequest {
        request: Some(search_request::Request::Query(proto::SearchQuery::default())),
    };
    assert!(matches!(
        s.search(&sync, request).unwrap(),
        Search::Query(_)
    ));
}

#[test]
fn search_network_accepts_records_above_local_send_limit_and_bounds_the_network() {
    let mut s = search_service();
    s.config.search.max_profile_bytes = 4096;
    s.config.search.min_profile_pow_bits = 0;
    let owner = Identity::generate();
    let profile = Profile {
        version: 1,
        age: 28,
        ..Default::default()
    };
    let record = padded_profile(&owner, &profile, 8192);
    let response = search(&s, search_request::Request::Publish(record.clone()));
    assert_eq!(
        (
            response.status,
            response.max_bytes,
            response.required_pow_bits
        ),
        (i32::from(Status::TooLarge), 4096, 0)
    );
    let request = SearchRequest {
        request: Some(search_request::Request::Publish(record.clone())),
    };
    let response = s.search_pow_done(&peer("key"), &request, true).unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::TooLarge), 4096)
    );
    let wrap = |record| {
        SearchRequest {
            request: Some(search_request::Request::Publish(record)),
        }
        .encode_to_vec()
    };
    assert_eq!(gossip(&s, &wrap(record)), Verdict::Accept);
    let newer = padded_profile(
        &owner,
        &Profile {
            version: 2,
            ..profile.clone()
        },
        8192,
    );
    assert_eq!(s.search_apply(&newer).unwrap(), Caught::Stored);
    let profile = Profile {
        version: 3,
        ..profile
    };
    let over = padded_profile(&owner, &profile, SEARCH_ACCEPT_BYTES);
    assert!(over.encoded_len() > SEARCH_ACCEPT_BYTES);
    assert_eq!(gossip(&s, &wrap(over.clone())), Verdict::Ignore);
    assert_eq!(s.search_apply(&over).unwrap(), Caught::Dropped);
    let overhead = over.encoded_len() - over.payload.len();
    let exact = padded_profile(&owner, &profile, SEARCH_ACCEPT_BYTES - overhead);
    assert_eq!(exact.encoded_len(), SEARCH_ACCEPT_BYTES);
    assert_eq!(s.search_apply(&exact).unwrap(), Caught::Stored);
}

#[test]
fn low_pow_heartbeat_does_not_refresh_or_remove_the_stored_record() {
    let mut s = search_service();
    s.config.search.min_profile_pow_bits = 0;
    let owner = Identity::generate();
    let record = Profile {
        version: 1,
        ..Default::default()
    }
    .sign(&owner);
    assert_eq!(
        search(&s, search_request::Request::Publish(record.clone())).status,
        i32::from(Status::Ok)
    );
    let key = dyapp_identity::key_hash(&owner.public_key());
    let index = s.search.clone().unwrap();
    let counters = || {
        index
            .reader()
            .unwrap()
            .query_row(
                "SELECT `heartbeat`, `beat_day`, `beats` FROM `key` WHERE `key` = ?",
                [&key[..]],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .unwrap()
    };
    let before = counters();
    s.config.search.min_profile_pow_bits = 2;
    let heartbeat = beat(&owner, 1, chrono::Utc::now().timestamp().unsigned_abs());
    let from = peer("weak-heartbeat");
    let response = search_as(
        &s,
        &from,
        search_request::Request::Heartbeat(heartbeat.clone()),
    );
    assert_eq!(
        (
            response.status,
            response.required_pow_bits,
            response.server_time
        ),
        (i32::from(Status::Invalid), 2, 0)
    );
    assert!(!s.reputation.banned(&from.id));
    let data = SearchRequest {
        request: Some(search_request::Request::Heartbeat(heartbeat.clone())),
    }
    .encode_to_vec();
    assert_eq!(s.search_gossip(&data), Verdict::Ignore);
    assert_eq!(counters(), before);
    assert_eq!(index.records(&[key], usize::MAX).unwrap(), vec![record]);
    let unknown = Identity::generate();
    let response = search(
        &s,
        search_request::Request::Heartbeat(beat(
            &unknown,
            1,
            chrono::Utc::now().timestamp().unsigned_abs(),
        )),
    );
    assert_eq!(
        (response.status, response.required_pow_bits),
        (i32::from(Status::NotFound), 0)
    );
    let strong = Identity::generate();
    let record = proven(
        &strong,
        Profile {
            version: 1,
            ..Default::default()
        },
        2,
    );
    assert_eq!(
        search(&s, search_request::Request::Publish(record)).status,
        i32::from(Status::Ok)
    );
    assert_eq!(
        search(
            &s,
            search_request::Request::Heartbeat(beat(
                &strong,
                1,
                chrono::Utc::now().timestamp().unsigned_abs()
            ))
        )
        .status,
        i32::from(Status::Ok)
    );
    s.config.search.min_profile_pow_bits = 0;
    assert_eq!(
        search(&s, search_request::Request::Heartbeat(heartbeat)).status,
        i32::from(Status::Ok)
    );
    assert_eq!(counters().2, before.2 + 1);
}

#[test]
fn search_pow_hint_is_specific_to_insufficient_difficulty() {
    let s = search_service();
    let owner = Identity::generate();
    let weak = Profile {
        version: 1,
        ..Default::default()
    }
    .sign(&owner);
    let response = search(&s, search_request::Request::Publish(weak.clone()));
    assert_eq!(
        (
            response.status,
            response.required_pow_bits,
            response.server_time
        ),
        (
            i32::from(Status::Invalid),
            s.config.search.min_profile_pow_bits,
            0
        )
    );
    let malformed = Profile::default().sign(&owner);
    let response = search(&s, search_request::Request::Publish(malformed));
    assert_eq!(
        (
            response.status,
            response.required_pow_bits,
            response.max_bytes
        ),
        (i32::from(Status::Invalid), 0, 0)
    );
    let mut forged = weak;
    forged.payload.push(0);
    let response = search(&s, search_request::Request::Publish(forged));
    assert_eq!(
        (response.status, response.required_pow_bits),
        (i32::from(Status::Denied), 0)
    );
}

#[test]
fn search_refusals_and_gossip() {
    let unsupported = service()
        .search(&peer("p"), SearchRequest::default())
        .unwrap();
    assert!(matches!(unsupported, Search::Reply(r) if search_status(&r) == Status::Unsupported));
    let s = search_service();
    let unknown = s.search(&peer("p"), SearchRequest::default()).unwrap();
    assert!(matches!(unknown, Search::Reply(r) if search_status(&r) == Status::Unsupported));
    let id = Identity::generate();
    let mut forged = Profile {
        version: 1,
        ..Profile::default()
    }
    .sign(&id);
    forged.payload.push(0);
    let r = search(&s, search_request::Request::Publish(forged.clone()));
    assert_eq!(search_status(&r), Status::Denied);
    let big = Profile {
        version: 1,
        place: "x".repeat(1000),
        interests: vec!["y".repeat(900); 80],
        ..Profile::default()
    }
    .sign(&id);
    let r = search(&s, search_request::Request::Publish(big));
    assert_eq!(search_status(&r), Status::TooLarge);
    let wrap = |r: SignedRecord| {
        SearchRequest {
            request: Some(search_request::Request::Publish(r)),
        }
        .encode_to_vec()
    };
    assert_eq!(gossip(&s, &wrap(forged)), Verdict::Reject);
    assert_eq!(gossip(&s, b"junk"), Verdict::Reject);
    let good = proven(
        &id,
        Profile {
            version: 1,
            ..Profile::default()
        },
        2,
    );
    assert_eq!(gossip(&s, &wrap(good.clone())), Verdict::Accept);
    assert_eq!(gossip(&s, &wrap(good)), Verdict::Ignore);
    s.traffic.add(s.config.limits.bytes_per_second);
    let q = proto::SearchQuery::default();
    let r = search(&s, search_request::Request::Query(q));
    assert_eq!(search_status(&r), Status::RateLimited);
}

#[test]
fn search_profile_pow() {
    // One strike bans, so both a missing strike and a given one are visible.
    let mut s = service_with(|l| {
        l.bytes_per_second = 1 << 20;
        l.strikes_to_ban = 1;
    });
    s.config.roles.push(Role::Search);
    s.config.search.min_profile_pow_bits = 2;
    let index = SearchIndex::open(&s.config.storage.search_path(), s.config.search.max_bytes);
    s.search = Some(Arc::new(index.unwrap()));
    let id = Identity::generate();
    let v1 = || Profile {
        version: 1,
        ..Profile::default()
    };
    let publish = |who: &str, r: SignedRecord| {
        search_status(&search_as(
            &s,
            &peer(who),
            search_request::Request::Publish(r),
        ))
    };
    // Below the minimum: INVALID, no strike.
    assert_eq!(publish("weak", proven(&id, v1(), 1)), Status::Invalid);
    assert!(!s.reputation.banned("weak"));
    // A 20-bit claim at nonce 0 holds with probability 2^-20: DENIED and a strike.
    let forged = Profile {
        pow_bits: 20,
        ..v1()
    };
    assert_eq!(publish("liar", forged.sign(&id)), Status::Denied);
    assert!(s.reputation.banned("liar"));
    // Valid: OK; the tombstone keeping the same proof needs no Argon2.
    let good = proven(&id, v1(), 2);
    assert_eq!(publish("good", good.clone()), Status::Ok);
    let (checks, _) = s.pow.take_counts();
    assert!(checks >= 1);
    let nonce = dyapp_profile::verify(&good).unwrap().profile.pow_nonce;
    let tomb = Profile::tombstone(2, nonce, 2).sign(&id);
    assert_eq!(s.search_apply(&tomb).unwrap(), Caught::Stored);
    assert_eq!(s.pow.take_counts(), (0, 0));
    // Gossip: below the minimum is ignored, a forged proof rejected.
    let wrap = |r: SignedRecord| {
        SearchRequest {
            request: Some(search_request::Request::Publish(r)),
        }
        .encode_to_vec()
    };
    let other = Identity::generate();
    assert_eq!(s.search_gossip(&wrap(v1().sign(&other))), Verdict::Ignore);
    let lie = Profile {
        pow_bits: 20,
        ..v1()
    };
    assert_eq!(gossip(&s, &wrap(lie.sign(&other))), Verdict::Reject);
    assert_eq!(gossip(&s, &wrap(proven(&other, v1(), 2))), Verdict::Accept);
}

#[test]
fn search_peers_sync_on_their_own_budget() {
    let s = search_service();
    let id = Identity::generate();
    let record = proven(
        &id,
        Profile {
            version: 1,
            ..Profile::default()
        },
        2,
    );
    search(&s, search_request::Request::Publish(record));
    let newer = proven(
        &id,
        Profile {
            version: 2,
            age: 40,
            ..Profile::default()
        },
        2,
    );
    // Inside 10 minutes: catch-up skips the update limit.
    assert_eq!(s.search_apply(&newer).unwrap(), Caught::Stored);
    let key = search_key(&SearchRequest {
        request: Some(search_request::Request::Publish(newer.clone())),
    })
    .unwrap();
    let sync = peer("sync");
    s.search_peer(&sync.id, true);
    let get = |peer: &Peer| {
        let keys = vec![key.to_vec()];
        search_as(
            &s,
            peer,
            search_request::Request::Get(proto::SearchGet { keys, puzzle: None }),
        )
    };
    // Far past the client budget of `requests_per_second`.
    for _ in 0..1_000 {
        assert_eq!(get(&sync).records, vec![newer.clone()]);
    }
    let client = peer("client");
    assert!((0..1_000).any(|_| search_status(&get(&client)) == Status::RateLimited));
    let inventory = |peer: &Peer, topic| {
        let request = InventoryRequest {
            topic,
            after: vec![],
        };
        s.search_inventory(peer, request).unwrap()
    };
    let have = inventory(&sync, crate::search::topic(&key)).have;
    assert_eq!(have[0].key, key.to_vec());
    assert_eq!(status_of(inventory(&sync, 16).status), Status::Invalid);
    s.search_sync
        .add(u64::from(s.config.maintenance.io_mb_per_s) << 20);
    assert_eq!(search_status(&get(&sync)), Status::RateLimited);
    assert_eq!(status_of(inventory(&sync, 0).status), Status::RateLimited);
    s.search_peer(&sync.id, false);
    assert!(!s.search_peers.lock().unwrap().contains(&sync.id));
}

fn beat_with(id: &Identity, time: u64, day: u32, nonce: u64, bits: u32) -> SignedRecord {
    let beat = proto::Heartbeat {
        time,
        version: 1,
        pow_nonce: nonce,
        pow_bits: bits,
        pow_day: day,
    };
    id.sign(Domain::Heartbeat, beat.encode_to_vec())
}

/// A search service with one indexed profile of `id` and 2-bit profile and heartbeat proofs.
fn beat_service(id: &Identity) -> Service {
    let mut s = service_with(|l| {
        l.bytes_per_second = 1 << 20;
        l.strikes_to_ban = 1;
    });
    s.config.roles.push(Role::Search);
    s.config.search.min_profile_pow_bits = 2;
    s.config.search.min_heartbeat_pow_bits = 2;
    let index = SearchIndex::open(&s.config.storage.search_path(), s.config.search.max_bytes);
    s.search = Some(Arc::new(index.unwrap()));
    let v1 = Profile {
        version: 1,
        ..Profile::default()
    };
    let r = search(&s, search_request::Request::Publish(proven(id, v1, 2)));
    assert_eq!(search_status(&r), Status::Ok);
    s
}

#[test]
fn search_heartbeat_pow() {
    let id = Identity::generate();
    let s = beat_service(&id);
    let send = |who: &str, r: SignedRecord| {
        search_status(&search_as(
            &s,
            &peer(who),
            search_request::Request::Heartbeat(r),
        ))
    };
    let now = chrono::Utc::now().timestamp().unsigned_abs();
    let day = u32::try_from(now / 86_400).unwrap();
    let nonce = dyapp_profile::solve_heartbeat(&id.public_key(), day, 2, 0, 100_000).unwrap();
    let other = dyapp_profile::solve_heartbeat(&id.public_key(), day - 1, 2, 0, 100_000).unwrap();
    s.pow.take_counts();
    // Missing, too weak or for another day: INVALID, no strike, no Argon2.
    assert_eq!(send("none", beat(&id, 1, now)), Status::Invalid);
    assert_eq!(
        send("weak", beat_with(&id, now, day, nonce, 1)),
        Status::Invalid
    );
    assert_eq!(
        send("old", beat_with(&id, now, day - 1, other, 2)),
        Status::Invalid
    );
    assert!(!s.reputation.banned("none") && !s.reputation.banned("old"));
    assert_eq!(s.pow.take_counts(), (0, 0));
    // A 20-bit claim at nonce 0 holds with probability 2^-20: DENIED and a strike.
    assert_eq!(
        send("liar", beat_with(&id, now, day, 0, 20)),
        Status::Denied
    );
    assert!(s.reputation.banned("liar"));
    // Valid: OK; the same proof again is cached.
    assert_eq!(send("good", beat_with(&id, now, day, nonce, 2)), Status::Ok);
    s.pow.take_counts();
    assert_eq!(
        send("good", beat_with(&id, now + 1, day, nonce, 2)),
        Status::Ok
    );
    assert_eq!(s.pow.take_counts(), (0, 0));
    // Past the day limit: RATE_LIMITED before any proof check, even a forged one.
    assert_eq!(
        send("good", beat_with(&id, now, day, 0, 20)),
        Status::RateLimited
    );
    assert_eq!(s.pow.take_counts(), (0, 0));
}

#[test]
fn heartbeat_just_before_midnight_passes() {
    let id = Identity::generate();
    let s = beat_service(&id);
    let day = u32::try_from(chrono::Utc::now().timestamp() / 86_400).unwrap() + 1;
    let now = i64::from(day) * 86_400 + 86_370;
    let nonce = dyapp_profile::solve_heartbeat(&id.public_key(), day, 2, 0, 100_000).unwrap();
    let record = beat_with(&id, now.unsigned_abs(), day, nonce, 2);
    assert!(matches!(
        s.beat_search_at(&record, now).unwrap(),
        Applied::Pow(c) if c.day == Some(day)
    ));
    let check = PowCheck {
        key: id.public_key(),
        nonce,
        bits: 2,
        day: Some(day),
    };
    assert!(s.pow.check(&check));
    assert!(matches!(
        s.beat_search_at(&record, now).unwrap(),
        Applied::Done
    ));
}
