use super::pow::PowDone;
use super::{
    clock_instant, refill, send_catch_up, Behaviour, Caught, OutboundRequestId, PeerId, Service,
    SignedRecord, Swarm,
};
use crate::catch_up::Asks;
use crate::pow::PowCheck;
use dyapp_p2p_net::proto;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

pub(super) struct InventoryState<'a> {
    pub(super) asks: &'a mut Asks,
    pub(super) queue: &'a mut HashMap<PeerId, VecDeque<[u8; 32]>>,
    pub(super) at: &'a mut tokio::time::Instant,
    pub(super) refill: &'a mut refill::Round,
}

pub(super) struct SearchState<'a> {
    pub(super) queue: &'a mut HashMap<PeerId, VecDeque<[u8; 32]>>,
    pub(super) asked: &'a mut HashMap<OutboundRequestId, (PeerId, Vec<[u8; 32]>)>,
    pub(super) at: &'a mut tokio::time::Instant,
}

pub(super) fn inventory_response(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    state: InventoryState<'_>,
    id: PeerId,
    request_id: OutboundRequestId,
    response: &proto::InventoryResponse,
) {
    let InventoryState {
        asks,
        queue,
        at,
        refill,
    } = state;
    let (Some(range), Some(index)) = (asks.range(&request_id), &service.search) else {
        return;
    };
    refill.answered();
    let page = response
        .have
        .iter()
        .filter_map(|have| Some((have.key.as_slice().try_into().ok()?, have.version)));
    let (have, next) = crate::catch_up::on_inventory(&range, page.collect());
    let keys: Vec<[u8; 32]> = have.iter().map(|have| have.0).collect();
    match index.versions(&keys) {
        Ok(local) => {
            let queue = queue.entry(id).or_default();
            // ponytail: O(n) contains per key; a page holds at most 10 000 keys.
            for ((key, version), local) in have.iter().zip(local) {
                if local.is_none_or(|local| local < *version) && !queue.contains(key) {
                    queue.push_back(*key);
                }
            }
            *at = (*at).min(clock_instant(service));
        }
        Err(error) => tracing::error!(%error, "search catch-up failed"),
    }
    if let Some(next) = next {
        asks.next_page(id, next);
    }
    asks.send(swarm);
}

pub(super) fn search_response(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    state: SearchState<'_>,
    permits: &Arc<tokio::sync::Semaphore>,
    done: &tokio::sync::mpsc::UnboundedSender<PowDone>,
    request_id: OutboundRequestId,
    response: &proto::SearchResponse,
) {
    let SearchState { queue, asked, at } = state;
    let Some((id, keys)) = asked.remove(&request_id) else {
        return;
    };
    let (got, stored, pending) = apply_records(service, &response.records);
    if !pending.is_empty() {
        spawn_pow(service, permits, done, pending);
    }
    tracing::debug!(stored, "search catch-up records applied");
    if !got.is_empty() {
        queue
            .entry(id)
            .or_default()
            .extend(keys.into_iter().filter(|key| !got.contains(key)));
    }
    if let Some(next) = send_catch_up(
        swarm,
        &service.rebuild_budget,
        queue,
        asked,
        clock_instant(service),
    ) {
        *at = next;
    }
}

fn apply_records(
    service: &Service,
    records: &[SignedRecord],
) -> (HashSet<[u8; 32]>, usize, Vec<(PowCheck, SignedRecord)>) {
    let mut got = HashSet::new();
    let mut stored = 0;
    let mut pending = Vec::new();
    for record in records {
        if let Ok(key) = <[u8; 32]>::try_from(record.public_key.as_slice()) {
            got.insert(dyapp_identity::key_hash(&key));
        }
        match service.search_apply(record) {
            Ok(Caught::Stored) => stored += 1,
            Ok(Caught::Dropped) => {}
            Ok(Caught::Pow(check)) => pending.push((check, record.clone())),
            Err(error) => tracing::error!(%error, "search catch-up record not stored"),
        }
    }
    (got, stored, pending)
}

fn spawn_pow(
    service: &Service,
    permits: &Arc<tokio::sync::Semaphore>,
    done: &tokio::sync::mpsc::UnboundedSender<PowDone>,
    pending: Vec<(PowCheck, SignedRecord)>,
) {
    let (permits, cache, done) = (permits.clone(), service.pow.clone(), done.clone());
    tokio::spawn(async move {
        let Ok(permit) = permits.acquire_owned().await else {
            return;
        };
        let _ = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let records = pending
                .into_iter()
                .filter(|(check, _)| cache.check(check))
                .map(|(_, record)| record)
                .collect();
            let _ = done.send(PowDone::CatchUp { records });
        })
        .await;
    });
}

pub(super) fn search_failure(
    swarm: &mut Swarm<Behaviour>,
    queue: &mut HashMap<PeerId, VecDeque<[u8; 32]>>,
    asked: &mut HashMap<OutboundRequestId, (PeerId, Vec<[u8; 32]>)>,
    id: PeerId,
    request_id: OutboundRequestId,
) {
    if let Some((_, keys)) = asked.remove(&request_id) {
        if swarm.is_connected(&id) {
            queue.entry(id).or_default().extend(keys);
        }
    }
}

pub(super) fn inventory_failure(
    swarm: &mut Swarm<Behaviour>,
    asks: &mut Asks,
    request_id: OutboundRequestId,
) {
    asks.range(&request_id);
    asks.send(swarm);
}

pub(super) fn digest(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    asks: &mut Asks,
    refill: &mut refill::Round,
    event: libp2p::request_response::Event<proto::DigestRequest, proto::DigestResponse>,
) {
    let Some(index) = &service.search else {
        return;
    };
    if asks.digest(swarm, index, event) {
        refill.answered();
    }
}
