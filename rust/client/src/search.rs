//! Profile search across several search nodes. Each node is untrusted and holds only a sample of
//! the index, so the client asks all of them, fetches what its cache lacks and re-checks every
//! record with [`search_results::check`].

use crate::profiles::Profiles;
use crate::scores::Scores;
use crate::selection::Operation;
use crate::store::{self, Store};
use crate::{Error, Node};
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{self, Condition, Status};
use dyapp_p2p_net::search_results;
use dyapp_p2p_net::Protocols;
use dyapp_profile::VerifiedProfile;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Keys one get request may carry.
const GET_KEYS: usize = 50;

mod evidence;

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

pub(crate) fn response_ok(response: proto::SearchResponse) -> Result<proto::SearchResponse, Error> {
    if response.status == i32::from(Status::RateLimited) {
        return Err(Error::RateLimited(Duration::from_secs(
            response.retry_after,
        )));
    }
    match Status::try_from(response.status) {
        Ok(Status::Ok) => Ok(response),
        Ok(status) => Err(Error::Status(status)),
        Err(_) => Err(Error::Reply("unknown status")),
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
#[must_use]
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
    connect_search_nodes_with_scores(entry, protocols, max, &mut Scores::default()).await
}

/// Connect search nodes using the caller's persistent selection state.
pub async fn connect_search_nodes_with_scores(
    entry: &mut Node,
    protocols: Arc<Protocols>,
    max: usize,
    scores: &mut Scores,
) -> SearchNodes {
    let (mut nodes, mut infos, mut failures) = (Vec::new(), Vec::new(), Vec::new());
    for addr in entry.providers(dyapp_p2p_net::SEARCH_KEY, max).await {
        let started = Instant::now();
        let reached = match Node::connect(addr, protocols.clone()).await {
            Ok(mut node) => node.info().await.map(|info| (node, info)),
            Err(error) => Err(error),
        };
        match reached {
            Ok((node, info)) if info.roles.contains(&(proto::Role::Search as i32)) => {
                scores.success(node.peer(), Operation::Search, started.elapsed());
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

/// Profiles matching every condition, from up to `limit` distinct hits (0 means 50, at most 200)
/// of each node in `nodes`, with at least `min_pow_bits` of profile proof of work. A hit whose
/// version `cache` already holds is taken from the cache, not fetched, and its proof of work is
/// not hashed again. Across all nodes, each owner is hashed at most once.
/// Peers in `blocked` ([`Store::blocked_peers`]) are neither fetched nor returned.
pub async fn search(
    nodes: &mut [Node],
    conditions: &[Condition],
    limit: u32,
    min_pow_bits: u32,
    cache: &Profiles,
    blocked: &[String],
) -> SearchResults {
    search_with_scores(
        nodes,
        &mut Scores::default(),
        conditions,
        limit,
        min_pow_bits,
        cache,
        blocked,
    )
    .await
}

/// Search using the caller's persistent selection state.
pub async fn search_with_scores(
    nodes: &mut [Node],
    scores: &mut Scores,
    conditions: &[Condition],
    limit: u32,
    min_pow_bits: u32,
    cache: &Profiles,
    blocked: &[String],
) -> SearchResults {
    let (mut replies, mut cached) = (Vec::new(), Vec::new());
    let mut observed = evidence::Observed::default();
    let mut partial = nodes.is_empty();
    let mut failures = Vec::new();
    let hit_limit = hit_limit(limit);
    if nodes.is_empty() {
        failures.push(SearchFailure::NoNodes);
    }
    scores.register_nodes(nodes);
    let peers: Vec<_> = nodes.iter().map(Node::peer).collect();
    let order = scores.ranked(&peers, Operation::Search);
    let mut yields = Vec::new();
    for index in order {
        let Some(node) = nodes.get_mut(index) else {
            continue;
        };
        let call = SearchCall::new(conditions, limit, hit_limit, cache, blocked);
        let result = search_node(
            node,
            scores,
            &call,
            &mut observed,
            &mut failures,
            &mut cached,
        )
        .await;
        partial |= result.partial;
        if let Some(count) = result.count {
            yields.push((node.peer(), count, result.cost));
        }
        replies.push(result.records);
    }
    record_zero_yields(scores, yields);
    let mut found = search_results::check(conditions, min_pow_bits, &replies);
    merge_cached(&mut found, cached, cache);
    found.retain(|profile| result_matches(profile, blocked, &observed, conditions, min_pow_bits));
    SearchResults {
        profiles: found,
        partial,
        failures,
    }
}

fn hit_limit(limit: u32) -> usize {
    (match limit {
        0 => 50,
        n => n.min(200),
    }) as usize
}

fn record_zero_yields(scores: &mut Scores, yields: Vec<(libp2p::PeerId, usize, Duration)>) {
    if yields.iter().any(|(_, count, _)| *count > 0) {
        for (peer, count, cost) in yields {
            if count == 0 {
                scores.zero_yield(peer, Operation::Search, cost);
            }
        }
    }
}

struct SearchCall<'a> {
    conditions: &'a [Condition],
    limit: u32,
    hit_limit: usize,
    active: bool,
    age: bool,
    cache: &'a Profiles,
    blocked: &'a [String],
}

impl<'a> SearchCall<'a> {
    fn new(
        conditions: &'a [Condition],
        limit: u32,
        hit_limit: usize,
        cache: &'a Profiles,
        blocked: &'a [String],
    ) -> Self {
        let needs = |field| conditions.iter().any(|condition| condition.field == field);
        Self {
            conditions,
            limit,
            hit_limit,
            active: needs(search_results::ACTIVE_DAY),
            age: needs(search_results::BIRTH_AGE),
            cache,
            blocked,
        }
    }
}

struct SearchYield {
    records: Vec<SignedRecord>,
    partial: bool,
    count: Option<usize>,
    cost: Duration,
}

async fn search_node(
    node: &mut Node,
    scores: &mut Scores,
    call: &SearchCall<'_>,
    observed: &mut evidence::Observed,
    failures: &mut Vec<SearchFailure>,
    cached: &mut Vec<VerifiedProfile>,
) -> SearchYield {
    let peer = node.peer();
    let started = Instant::now();
    let reply = match query_node(node, scores, call, failures).await {
        Ok(reply) => reply,
        Err(failure) => return failure,
    };
    let mut partial = call.partial(&reply);
    let mut valid = true;
    if reply.hits.len() > call.hit_limit {
        scores.invalid(peer, Operation::Search);
        failures.push(SearchFailure::Invalid);
        partial = true;
        valid = false;
    }
    let keys = cache_keys(reply.hits, call.hit_limit, call.cache, call.blocked, cached);
    let (records, incomplete) = fetch(node, keys, failures).await;
    partial |= incomplete;
    if incomplete {
        scores.fault(peer, crate::now(), Operation::Search);
    }
    // `check` drops such a record anyway; the node that sent it is not to be trusted.
    if !observed.observe(&records) {
        scores.invalid(peer, Operation::Search);
        failures.push(SearchFailure::Invalid);
        valid = false;
    }
    if valid {
        scores.success(peer, Operation::Search, started.elapsed());
    }
    SearchYield {
        count: Some(records.len()),
        records,
        partial,
        cost: started.elapsed(),
    }
}

impl SearchYield {
    fn failure(cost: Duration) -> Self {
        Self {
            records: Vec::new(),
            partial: true,
            count: None,
            cost,
        }
    }
}

async fn query_node(
    node: &mut Node,
    scores: &mut Scores,
    call: &SearchCall<'_>,
    failures: &mut Vec<SearchFailure>,
) -> Result<proto::SearchResponse, SearchYield> {
    let peer = node.peer();
    let started = Instant::now();
    let reply = match node
        .search_query(call.conditions.to_vec(), call.limit)
        .await
    {
        Ok(reply) => reply,
        Err(error) => {
            if crate::scores::faulty(&error) {
                scores.fault(peer, crate::now(), Operation::Search);
            } else {
                scores.not_found(peer, Operation::Search, started.elapsed());
            }
            failures.push((&error).into());
            return Err(SearchYield::failure(started.elapsed()));
        }
    };
    if call.active && !reply.applied.contains(&search_results::ACTIVE_DAY) {
        scores.fault(peer, crate::now(), Operation::Search);
        return Err(SearchYield::failure(started.elapsed()));
    }
    Ok(reply)
}

impl SearchCall<'_> {
    fn partial(&self, reply: &proto::SearchResponse) -> bool {
        reply.partial || (self.age && !reply.applied.contains(&search_results::BIRTH_AGE))
    }
}

fn cache_keys(
    hits: Vec<proto::SearchHit>,
    limit: usize,
    cache: &Profiles,
    blocked: &[String],
    cached: &mut Vec<VerifiedProfile>,
) -> Vec<Vec<u8>> {
    let mut unique: Vec<proto::SearchHit> = Vec::new();
    let mut indices: HashMap<Vec<u8>, usize> = HashMap::new();
    for hit in hits {
        if let Some(&index) = indices.get(&hit.key) {
            if let Some(existing) = unique.get_mut(index) {
                existing.version = existing.version.max(hit.version);
            }
        } else {
            indices.insert(hit.key.clone(), unique.len());
            unique.push(hit);
        }
    }
    let mut keys = Vec::new();
    for hit in unique.into_iter().take(limit) {
        let peer_id: String = dyapp_identity::hex(&hit.key);
        if blocked.contains(&peer_id) {
            continue;
        }
        match cache.get(&peer_id) {
            Some(held) if held.profile.version >= hit.version => cached.push(held.clone()),
            _ => keys.push(hit.key),
        }
    }
    keys
}

fn merge_cached(found: &mut Vec<VerifiedProfile>, cached: Vec<VerifiedProfile>, cache: &Profiles) {
    // A node may serve an older version than the cache holds.
    for profile in &mut *found {
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
}

fn result_matches(
    profile: &VerifiedProfile,
    blocked: &[String],
    observed: &evidence::Observed,
    conditions: &[Condition],
    min_pow_bits: u32,
) -> bool {
    !blocked.contains(&profile.peer_id)
        && observed.permits(profile)
        && search_results::passes(conditions, min_pow_bits, &profile.profile)
}

/// Sends the profile [`Store::hold_search_profile`] holds to every node in `nodes` (search nodes
/// from [`Node::providers`] with [`dyapp_p2p_net::SEARCH_KEY`], two are enough since they gossip
/// it on) once its window has ended at Unix second `now`. Returns whether a node took it; else
/// it stays held, for the `RATE_LIMITED` wait if a node gave one, and is sent on a later call.
///
/// # Errors
///
/// Returns an error if reading or updating the held-publication state fails or
/// its stored encoding is invalid. Node request failures are retained for retry
/// rather than returned; a node may already have accepted the profile.
pub async fn publish_due(
    store: &Store,
    nodes: &mut [Node],
    now: u64,
) -> Result<bool, store::Error> {
    publish_due_with_scores(store, nodes, &mut Scores::default(), now).await
}

/// Publish using the caller's persistent selection state.
///
/// # Errors
/// Returns the same storage and held-publication failures as [`publish_due`].
pub async fn publish_due_with_scores(
    store: &Store,
    nodes: &mut [Node],
    scores: &mut Scores,
    now: u64,
) -> Result<bool, store::Error> {
    let Some(record) = store.due_search_publication(now)? else {
        return Ok(false);
    };
    publish_record(store, nodes, scores, record, now).await
}

/// Retry only the exact explicitly confirmed recovery record, without signing or confirming.
/// Unsupported nodes leave it queued; a changed owner context or head is refused.
///
/// # Errors
/// Returns changed review, malformed state or storage errors.
pub async fn publish_recovery(
    store: &Store,
    plan: &store::SearchWithdrawalRecovery,
    nodes: &mut [Node],
    now: u64,
) -> Result<bool, store::Error> {
    publish_recovery_with_scores(store, plan, nodes, &mut Scores::default(), now).await
}

/// Publish recovery using the caller's persistent selection state.
///
/// # Errors
/// Returns the same review, malformed-state and storage failures as [`publish_recovery`].
pub async fn publish_recovery_with_scores(
    store: &Store,
    plan: &store::SearchWithdrawalRecovery,
    nodes: &mut [Node],
    scores: &mut Scores,
    now: u64,
) -> Result<bool, store::Error> {
    let Some(record) = store.due_search_withdrawal_recovery(plan, now)? else {
        return Ok(false);
    };
    publish_record(store, nodes, scores, record, now).await
}

async fn publish_record(
    store: &Store,
    nodes: &mut [Node],
    scores: &mut Scores,
    record: SignedRecord,
    now: u64,
) -> Result<bool, store::Error> {
    let profile = dyapp_profile::verify_search(&record)
        .map_err(|_| store::Error::InvalidSetting)?
        .profile;
    let (mut taken, mut wait) = (false, None);
    scores.register_nodes(nodes);
    for node in nodes {
        let started = Instant::now();
        let result = if profile.search_only {
            node.search_withdraw(record.clone()).await
        } else {
            node.search_publish(record.clone()).await
        };
        match result {
            // A node that holds this version already took it.
            Ok(()) | Err(Error::Status(Status::Stale)) => {
                scores.success(node.peer(), Operation::ProfilePublish, started.elapsed());
                taken = true;
            }
            Err(Error::RateLimited(after)) => {
                scores.fault(node.peer(), crate::now(), Operation::ProfilePublish);
                wait = wait.max(Some(after));
            }
            Err(error) => {
                if crate::scores::faulty(&error) {
                    scores.fault(node.peer(), crate::now(), Operation::ProfilePublish);
                } else {
                    scores.not_found(node.peer(), Operation::ProfilePublish, started.elapsed());
                }
            }
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
///
/// # Errors
///
/// Returns an error if visibility or heartbeat state cannot be read or decoded,
/// scheduling randomness fails, or saving the next heartbeat time fails. Nodes
/// may already have accepted a heartbeat before a later storage failure.
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
    let signed_time = crate::clock::corrected(now);
    let Ok(day) = u32::try_from(signed_time / 86_400) else {
        return Ok(false);
    };
    // shortcut: solves on the calling task (seconds at the default 8 bits); move to a blocking
    // thread if the UI thread ever awaits this.
    let Ok(nonce) = dyapp_profile::solve_heartbeat(&identity.public_key(), day, bits, 0, u64::MAX)
    else {
        return Ok(false);
    };
    let beat = proto::Heartbeat {
        time: signed_time,
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
mod tests;
