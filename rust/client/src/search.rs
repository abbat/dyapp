//! Profile search across several search nodes. Each node is untrusted and holds only a sample of
//! the index, so the client asks all of them, fetches what its cache lacks and re-checks every
//! record with [`search_results::check`].

use crate::profiles::Profiles;
use crate::store::{self, Store};
use crate::{Error, Node};
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{self, Condition, Status};
use dyapp_p2p_net::search_results;
use dyapp_p2p_net::Protocols;
use dyapp_profile::VerifiedProfile;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

/// Keys one get request may carry.
const GET_KEYS: usize = 50;

/// Verified profiles and whether search coverage may be incomplete.
pub struct SearchResults {
    /// Matches, one per owner at the highest seen version, in no set order.
    pub profiles: Vec<VerifiedProfile>,
    /// A node failed, skipped an uncheckable condition, omitted an age filter, or cut a reply.
    pub partial: bool,
    /// Why nodes failed, one entry per failed request, in the order they failed.
    pub failures: Vec<SearchFailure>,
}

/// A company filter applied only on this device to verified results; no node sees it, and it
/// cannot hide anyone's own public profile from colleagues.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Employer {
    /// Every profile.
    #[default]
    Any,
    /// Only profiles naming this company.
    Same(String),
    /// Profiles naming none of these companies; a profile without a company always passes.
    Exclude(Vec<String>),
}

impl Employer {
    /// Keep the `profiles` this filter allows, comparing company names without case and with
    /// whitespace collapsed.
    pub fn retain(&self, profiles: &mut Vec<VerifiedProfile>) {
        let key = |c: &str| {
            c.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        };
        match self {
            Self::Any => {}
            Self::Same(company) => {
                let company = key(company);
                profiles.retain(|p| !company.is_empty() && key(&p.profile.company) == company);
            }
            Self::Exclude(companies) => {
                let companies: HashSet<_> = companies.iter().map(|c| key(c)).collect();
                profiles.retain(|p| {
                    p.profile.company.trim().is_empty()
                        || !companies.contains(&key(&p.profile.company))
                });
            }
        }
    }
}

/// Why a search gave no usable reply from a node; each has its own message and retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFailure {
    /// No search node was given.
    NoNodes,
    /// The node could not be reached, the request broke, or the node would not serve it.
    Unavailable,
    /// The node did not reply within [`crate::TIMEOUT`].
    Timeout,
    /// The node is busy; ask again after this long.
    RateLimited(Duration),
    /// The node set a puzzle the client would not solve or still refused the solution.
    Puzzle,
    /// The reply was malformed or carried a record with a forged signature.
    Invalid,
}

impl From<&Error> for SearchFailure {
    fn from(error: &Error) -> Self {
        match error {
            Error::Timeout => Self::Timeout,
            Error::RateLimited(after) => Self::RateLimited(*after),
            Error::Status(Status::Puzzle)
            | Error::Reply("puzzle too hard" | "bad puzzle challenge") => Self::Puzzle,
            Error::Connect(_)
            | Error::Request(_)
            | Error::Status(
                Status::Denied | Status::Full | Status::Refused | Status::Unsupported,
            ) => Self::Unavailable,
            _ => Self::Invalid,
        }
    }
}

/// The condition fields a search over these nodes may offer, ascending: those every node lists in
/// `NodeInfo.search_fields` and the client can check ([`search_results::checkable`]). No nodes,
/// none.
pub fn offered_fields(infos: &[proto::NodeInfo]) -> Vec<u32> {
    let Some((first, rest)) = infos.split_first() else {
        return Vec::new();
    };
    let mut fields: Vec<u32> = first
        .search_fields
        .iter()
        .copied()
        .filter(|&field| {
            search_results::checkable(field)
                && rest.iter().all(|info| info.search_fields.contains(&field))
        })
        .collect();
    fields.sort_unstable();
    fields.dedup();
    fields
}

/// Search nodes reached through one entry node, ready for [`search`].
pub struct SearchNodes {
    /// Connected nodes that list the search role.
    pub nodes: Vec<Node>,
    /// [`offered_fields`] of `nodes`.
    pub offered: Vec<u32>,
    /// Why a provider was dropped, one entry per dropped provider.
    pub failures: Vec<SearchFailure>,
}

/// Up to `max` search providers that `entry` finds ([`dyapp_p2p_net::SEARCH_KEY`]), each
/// connected and asked for its info. A provider that fails or does not list the search role is
/// dropped.
pub async fn connect_search_nodes(
    entry: &mut Node,
    protocols: Arc<Protocols>,
    max: usize,
) -> SearchNodes {
    let (mut nodes, mut infos, mut failures) = (Vec::new(), Vec::new(), Vec::new());
    for addr in entry.providers(dyapp_p2p_net::SEARCH_KEY, max).await {
        let reached = match Node::connect(addr, protocols.clone()).await {
            Ok(mut node) => node.info().await.map(|info| (node, info)),
            Err(error) => Err(error),
        };
        match reached {
            Ok((node, info)) if info.roles.contains(&(proto::Role::Search as i32)) => {
                nodes.push(node);
                infos.push(info);
            }
            Ok(_) => failures.push(SearchFailure::Unavailable),
            Err(error) => failures.push((&error).into()),
        }
    }
    let offered = offered_fields(&infos);
    SearchNodes {
        nodes,
        offered,
        failures,
    }
}

/// Profiles matching every condition, from up to `limit` hits (0 means 50) of each node in
/// `nodes`, with at least `min_pow_bits` of profile proof of work. A hit whose version `cache`
/// already holds is taken from the cache, not fetched, and its proof of work is not hashed again.
/// Peers in `blocked` ([`Store::blocked_peers`]) are neither fetched nor returned.
pub async fn search(
    nodes: &mut [Node],
    conditions: &[Condition],
    limit: u32,
    min_pow_bits: u32,
    cache: &Profiles,
    blocked: &[String],
) -> SearchResults {
    let (mut replies, mut cached) = (Vec::new(), Vec::new());
    let mut partial = nodes.is_empty();
    let mut failures = Vec::new();
    if nodes.is_empty() {
        failures.push(SearchFailure::NoNodes);
    }
    let active = conditions
        .iter()
        .any(|c| c.field == search_results::ACTIVE_DAY);
    let age = conditions
        .iter()
        .any(|c| c.field == search_results::BIRTH_AGE);
    for node in nodes {
        let reply = match node.search_query(conditions.to_vec(), limit).await {
            Ok(reply) => reply,
            Err(error) => {
                failures.push((&error).into());
                partial = true;
                continue;
            }
        };
        partial |= reply.partial || (age && !reply.applied.contains(&search_results::BIRTH_AGE));
        // The client cannot re-check activity, so a node that skipped it adds nothing.
        if active && !reply.applied.contains(&search_results::ACTIVE_DAY) {
            partial = true;
            continue;
        }
        let mut keys = Vec::new();
        for hit in reply.hits {
            let peer_id: String = hit.key.iter().map(|byte| format!("{byte:02x}")).collect();
            if blocked.contains(&peer_id) {
                continue;
            }
            match cache.get(&peer_id) {
                Some(held) if held.profile.version >= hit.version => cached.push(held),
                _ => keys.push(hit.key),
            }
        }
        let (records, incomplete) = fetch(node, keys, &mut failures).await;
        partial |= incomplete;
        // `check` drops such a record anyway; the node that sent it is not to be trusted.
        if records.iter().any(|r| dyapp_profile::verify(r).is_err()) {
            failures.push(SearchFailure::Invalid);
        }
        replies.push(records);
    }
    let mut found = search_results::check(conditions, min_pow_bits, &replies);
    // A node may serve an older version than the cache holds.
    for profile in &mut found {
        match cache.get(&profile.peer_id) {
            Some(held) if held.profile.version > profile.profile.version => *profile = held.clone(),
            _ => {}
        }
    }
    for held in cached {
        if !found.iter().any(|profile| profile.peer_id == held.peer_id) {
            found.push(held.clone());
        }
    }
    found.retain(|profile| {
        !blocked.contains(&profile.peer_id)
            && search_results::passes(conditions, min_pow_bits, &profile.profile)
    });
    SearchResults {
        profiles: found,
        partial,
        failures,
    }
}

/// Sends the profile [`Store::hold_search_profile`] holds to every node in `nodes` (search nodes
/// from [`Node::providers`] with [`dyapp_p2p_net::SEARCH_KEY`], two are enough since they gossip
/// it on) once its window has ended at Unix second `now`. Returns whether a node took it; else
/// it stays held, for the `RATE_LIMITED` wait if a node gave one, and is sent on a later call.
pub async fn publish_due(
    store: &Store,
    nodes: &mut [Node],
    now: u64,
) -> Result<bool, store::Error> {
    let Some(record) = store.due_search_profile(now)? else {
        return Ok(false);
    };
    let (mut taken, mut wait) = (false, None);
    for node in nodes {
        match node.search_publish(record.clone()).await {
            // A node that holds this version already took it.
            Ok(()) | Err(Error::Status(Status::Stale)) => taken = true,
            Err(Error::RateLimited(after)) => wait = wait.max(Some(after)),
            Err(_) => {}
        }
    }
    if taken {
        store.search_profile_sent(&record, now)?;
    } else if let Some(after) = wait {
        store.search_profile_rate_limited(now, after)?;
    }
    Ok(taken)
}

// The records of `keys` that `node` returns, asking again for those a cut reply left out; a failed
// request goes to `failures`.
async fn fetch(
    node: &mut Node,
    keys: Vec<Vec<u8>>,
    failures: &mut Vec<SearchFailure>,
) -> (Vec<SignedRecord>, bool) {
    let mut records = Vec::new();
    let mut incomplete = false;
    for chunk in keys.chunks(GET_KEYS) {
        let mut wanted = chunk.to_vec();
        while !wanted.is_empty() {
            let reply = match node.search_get(wanted.clone()).await {
                Ok(reply) => reply,
                Err(error) => {
                    failures.push((&error).into());
                    incomplete = true;
                    break;
                }
            };
            let asked = wanted.len();
            for record in reply.records {
                let key = dyapp_identity::sha256(&record.public_key);
                if let Some(index) = wanted.iter().position(|wanted| *wanted == key) {
                    wanted.swap_remove(index);
                    records.push(record);
                }
            }
            if !reply.partial || wanted.len() == asked {
                incomplete |= !wanted.is_empty();
                break;
            }
        }
    }
    (records, incomplete)
}

/// Sends `identity`'s heartbeat for its profile `version` to every node in `nodes` once one is
/// due at Unix second `now` (at a random second of the day after the last), which keeps the
/// profile listed. `bits` is the
/// nodes' `NodeInfo.min_heartbeat_pow`. Returns whether a node took it; `RATE_LIMITED` means it
/// has two for today. On `NOT_FOUND` a node lacks this version, which the next publish fixes.
/// A hidden profile ([`Store::profile_hidden`]) sends none.
pub async fn beat_due(
    store: &Store,
    identity: &Identity,
    version: u64,
    bits: u32,
    nodes: &mut [Node],
    now: u64,
) -> Result<bool, store::Error> {
    if store.profile_hidden()? || !store.search_heartbeat_due(now)? {
        return Ok(false);
    }
    let Ok(day) = u32::try_from(now / 86_400) else {
        return Ok(false);
    };
    // shortcut: solves on the calling task (seconds at the default 8 bits); move to a blocking
    // thread if the UI thread ever awaits this.
    let Ok(nonce) = dyapp_profile::solve_heartbeat(&identity.public_key(), day, bits, 0, u64::MAX)
    else {
        return Ok(false);
    };
    let beat = proto::Heartbeat {
        time: now,
        version,
        pow_nonce: nonce,
        pow_bits: bits,
        pow_day: day,
    };
    let record = identity.sign(Domain::Heartbeat, prost::Message::encode_to_vec(&beat));
    let mut taken = false;
    for node in nodes {
        match node.search_heartbeat(record.clone()).await {
            Ok(()) | Err(Error::RateLimited(_)) => taken = true,
            Err(_) => {}
        }
    }
    if taken {
        store.search_heartbeat_sent(now)?;
    }
    Ok(taken)
}

#[cfg(test)]
mod tests {
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
        let mut server = build_configured_swarm(Keypair::generate_ed25519(), options).unwrap();
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
        let [a, b, c] = [(); 3].map(|()| Identity::generate());
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
            ints: vec![19991231, 20001231],
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
        assert!(super::offered_fields(&[]).is_empty());
    }

    #[tokio::test]
    async fn drops_nodes_that_skip_the_activity_condition() {
        let [a, b] = [(); 2].map(|()| Identity::generate());
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

    #[tokio::test]
    async fn merges_nodes_skips_cached_hits_and_resends_cut_gets() {
        let [a, b, c, d, e, f, g, h] = [(); 8].map(|()| Identity::generate());
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
        // 51 uncached hits: two gets. d was not asked for; e is cached newer and no longer fits.
        let mut hits: Vec<_> = (0..51u8).map(|n| hit(vec![n; 32], 1)).collect();
        // A whole reply that misses keys is not asked again.
        hits[0] = hit(key(&h), 1);
        hits.insert(0, hit(key(&e), 4));
        let mut second = vec![
            ok(hits, Vec::new(), false),
            ok(
                Vec::new(),
                vec![record(&d, 1, 30), record(&e, 4, 30), record(&h, 1, 30)],
                false,
            ),
            ok(Vec::new(), Vec::new(), false),
        ]
        .into_iter();
        let (two, two_asked) = fake(move |_| second.next().unwrap()).await;
        let (down, _) = fake(|_| SearchResponse {
            status: Status::Denied.into(),
            ..SearchResponse::default()
        })
        .await;
        let conditions = [Condition {
            field: search_results::BIRTH_AGE,
            op: Op::Range.into(),
            ints: vec![19901025, 20011025],
            strings: Vec::new(),
        }];
        let mut nodes = [one, two, down];
        // Blocked: b is cached and c would be fetched.
        let blocked = [b.peer_id(), c.peer_id()];
        let search = search(&mut nodes, &conditions, 0, 0, &cache, &blocked);
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
        let mut expected = vec![(a.peer_id(), 2), (f.peer_id(), 5), (h.peer_id(), 1)];
        expected.sort();
        assert_eq!(found, expected);
        let one_gets = gets(&one_asked);
        assert_eq!(one_gets.len(), 3);
        assert_eq!(one_gets[0], [key(&a), key(&f), key(&g)]);
        assert_eq!(one_gets[1].len(), 2);
        assert_eq!(one_gets[2], [key(&g)]);
        let two_gets = gets(&two_asked);
        assert_eq!(two_gets.iter().map(Vec::len).collect::<Vec<_>>(), [50, 1]);
        assert!(!two_gets.concat().contains(&key(&e)));
    }

    #[tokio::test]
    async fn classifies_failures_and_drops_forged_records() {
        let [a, b] = [(); 2].map(|()| Identity::generate());
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
        let mut nodes = [liar, busy, hard, broken, refusing];
        let cache = Profiles::default();
        let search = search(&mut nodes, &[], 0, 0, &cache, &[]);
        let found = tokio::time::timeout(Duration::from_secs(10), search)
            .await
            .unwrap();
        assert!(found.partial);
        let peers: Vec<_> = found.profiles.iter().map(|p| p.peer_id.clone()).collect();
        assert_eq!(peers, [a.peer_id()]);
        assert_eq!(
            found.failures,
            [
                SearchFailure::Invalid,
                SearchFailure::RateLimited(Duration::from_secs(30)),
                SearchFailure::Puzzle,
                SearchFailure::Invalid,
                SearchFailure::Unavailable,
            ]
        );
        let none = super::search(&mut [], &[], 0, 0, &cache, &[]).await;
        assert!(none.partial && none.profiles.is_empty());
        assert_eq!(none.failures, [SearchFailure::NoNodes]);
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

    async fn within(
        publish: impl std::future::Future<Output = Result<bool, store::Error>>,
    ) -> bool {
        tokio::time::timeout(Duration::from_secs(10), publish)
            .await
            .unwrap()
            .unwrap()
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
        let alice = Identity::generate();
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
        let mut nodes = [busy, slow];
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
        let alice = Identity::generate();
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
            let Request::Heartbeat(record) = &requests[2] else {
                panic!("not a heartbeat");
            };
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
            dyapp_profile::verify(&profile.sign(&Identity::generate())).unwrap()
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
        assert!(kept(Employer::Same("  ".into())).is_empty());
        assert_eq!(
            kept(Employer::Exclude(vec!["ACME CORP".into(), "".into()])),
            ["", "Other"]
        );
    }
}
