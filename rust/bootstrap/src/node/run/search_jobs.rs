//! Bounded background search work for the swarm loop.

use super::pow::PowDone;
use super::{
    drop_banned, jobs, peer, run_get, run_query, search_key, spread, Behaviour, ConnectionId, Peer,
    PeerId, ResponseChannel, Search, SearchResponse, Service, Status, Swarm,
};
use dyapp_p2p_net::proto;
use prost::Message;
use std::sync::Arc;

pub(super) struct SearchJob {
    pub(super) from: Peer,
    pub(super) channel: ResponseChannel<SearchResponse>,
    pub(super) work: Search,
    pub(super) queued: std::time::Instant,
}

pub(super) struct SearchDone {
    pub(super) from: Peer,
    pub(super) channel: ResponseChannel<SearchResponse>,
    pub(super) get: bool,
    pub(super) reply: crate::Result<SearchResponse>,
}

const SEARCH_WORKERS: usize = 2;

pub(super) fn start_search(
    queue: &mut jobs::GroupQueue<SearchJob>,
    running: &mut usize,
    service: &Service,
    done: &tokio::sync::mpsc::UnboundedSender<SearchDone>,
) {
    let Some(index) = &service.search else { return };
    while *running < SEARCH_WORKERS {
        let Some(SearchJob {
            from,
            channel,
            work,
            queued,
        }) = queue.pop()
        else {
            return;
        };
        service.puzzle.record_wait(queued.elapsed());
        *running = (*running).saturating_add(1);
        let (index, done) = (index.clone(), done.clone());
        let rows = service.config.search.max_scan_rows;
        tokio::task::spawn_blocking(move || {
            let (get, reply) = match work {
                Search::Get(keys) => (true, run_get(&index, &keys)),
                Search::Query(query) => (false, run_query(&index, &query, rows)),
                Search::Reply(_) | Search::Pow(..) => unreachable!("answered on the loop"),
            };
            let _ = done.send(SearchDone {
                from,
                channel,
                get,
                reply,
            });
        });
    }
}

pub(super) struct RequestContext<'a> {
    pub(super) swarm: &'a mut Swarm<Behaviour>,
    pub(super) service: &'a Service,
    pub(super) queue: &'a mut jobs::GroupQueue<SearchJob>,
    pub(super) running: &'a mut usize,
    pub(super) done: &'a tokio::sync::mpsc::UnboundedSender<SearchDone>,
    pub(super) pow_permits: &'a Arc<tokio::sync::Semaphore>,
    pub(super) pow_done: &'a tokio::sync::mpsc::UnboundedSender<PowDone>,
}

#[derive(Default)]
pub(super) struct RequestOutcome {
    pub(super) answered: u64,
    pub(super) failed: u64,
}

pub(super) fn request(
    mut context: RequestContext<'_>,
    id: PeerId,
    connection_id: ConnectionId,
    request: proto::SearchRequest,
    channel: ResponseChannel<SearchResponse>,
) -> RequestOutcome {
    let gossip = search_key(&request).map(|key| (key, request.encode_to_vec()));
    let solution = match &request.request {
        Some(proto::search_request::Request::Query(query)) => query.puzzle.clone(),
        Some(proto::search_request::Request::Get(get)) => get.puzzle.clone(),
        _ => None,
    };
    let from = peer(context.swarm, context.service, id, connection_id);
    let mut outcome = RequestOutcome::default();
    match context.service.search(&from, request) {
        Ok(Search::Reply(response)) => {
            outcome.answered = outcome.answered.saturating_add(1);
            spread(context.swarm, gossip, &response, context.service);
            context.service.traffic.add(response.encoded_len() as u64);
            let _ = context
                .swarm
                .behaviour_mut()
                .search
                .send_response(channel, response);
        }
        Ok(Search::Pow(check, request)) => {
            outcome.answered = outcome.answered.saturating_add(check_pow(
                &mut context,
                id,
                from,
                channel,
                check,
                request,
            ));
        }
        Ok(work) => {
            outcome.answered = outcome.answered.saturating_add(queue_work(
                &mut context,
                from,
                channel,
                work,
                solution.as_ref(),
            ));
        }
        Err(error) => {
            outcome.failed = outcome.failed.saturating_add(1);
            tracing::error!(%error, "search request failed");
        }
    }
    drop_banned(context.swarm, context.service, id);
    outcome
}

fn check_pow(
    context: &mut RequestContext<'_>,
    id: PeerId,
    from: Peer,
    channel: ResponseChannel<SearchResponse>,
    check: crate::pow::PowCheck,
    request: proto::SearchRequest,
) -> u64 {
    let Ok(permit) = context.pow_permits.clone().try_acquire_owned() else {
        rate_limited(context.swarm, channel);
        return 1;
    };
    let (cache, done) = (context.service.pow.clone(), context.pow_done.clone());
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let ok = cache.check(&check);
        let _ = done.send(PowDone::Publish {
            peer: id,
            from,
            channel,
            request,
            ok,
        });
    });
    0
}

fn queue_work(
    context: &mut RequestContext<'_>,
    from: Peer,
    channel: ResponseChannel<SearchResponse>,
    work: Search,
    solution: Option<&proto::Puzzle>,
) -> u64 {
    let group = (!context.service.is_search_peer(&from)).then(|| from.group.clone());
    let peer_id = from.id.clone();
    let job = SearchJob {
        from,
        channel,
        work,
        queued: context.service.clock.instant(),
    };
    let answered = match context.queue.push(group.as_deref(), job) {
        Ok(()) => {
            if let Some(solution) = solution {
                context.service.puzzle.consume(&peer_id, solution);
            }
            0
        }
        Err(SearchJob { channel, .. }) => {
            rate_limited(context.swarm, channel);
            1
        }
    };
    start_search(
        context.queue,
        context.running,
        context.service,
        context.done,
    );
    answered
}

fn rate_limited(swarm: &mut Swarm<Behaviour>, channel: ResponseChannel<SearchResponse>) {
    let response = SearchResponse {
        status: Status::RateLimited.into(),
        retry_after: 1,
        ..SearchResponse::default()
    };
    let _ = swarm
        .behaviour_mut()
        .search
        .send_response(channel, response);
}
