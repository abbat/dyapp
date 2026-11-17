use super::pow::PowDone;
use super::search_jobs::start_search;
use super::state::State;
use super::{pow, stored, Arc, Behaviour, Finished, SearchResponse, Service, Status, Swarm};
use prost::Message as _;

pub(super) fn store(
    state: &mut State,
    swarm: &mut Swarm<Behaviour>,
    service: &Arc<Service>,
    Finished {
        peer: id,
        connection,
        nonce,
        done,
    }: Finished,
) {
    state.store_queue.done(id);
    if let Some(nonce) = nonce.filter(|_| swarm.behaviour().endpoints.ip(connection).is_some()) {
        state.nonces.insert(connection, nonce);
    }
    let Some(done) = done else {
        state.failed = state.failed.saturating_add(1);
        tracing::error!(peer = %id, "store request panicked");
        return;
    };
    if stored::reply(
        swarm,
        service,
        &mut state.replicas,
        &mut state.watchers,
        (id, connection),
        done,
    ) {
        state.answered = state.answered.saturating_add(1);
    } else {
        state.failed = state.failed.saturating_add(1);
    }
}

pub(super) fn search(
    state: &mut State,
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    done: super::search_jobs::SearchDone,
) {
    state.searching = state.searching.saturating_sub(1);
    start_search(
        &mut state.search_queue,
        &mut state.searching,
        service,
        &state.channels.search_done_tx,
    );
    let response = match done.reply {
        Ok(reply) if done.get && !service.charge_get(&done.from, reply.encoded_len()) => {
            SearchResponse {
                status: Status::RateLimited.into(),
                retry_after: 1,
                ..SearchResponse::default()
            }
        }
        Ok(reply) => reply,
        Err(error) => {
            state.failed = state.failed.saturating_add(1);
            tracing::error!(%error, "search failed");
            return;
        }
    };
    state.answered = state.answered.saturating_add(1);
    service.traffic.add(response.encoded_len() as u64);
    let _ = swarm
        .behaviour_mut()
        .search
        .send_response(done.channel, response);
}

pub(super) fn pow_done(
    state: &mut State,
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    done: PowDone,
) {
    match done {
        PowDone::Publish {
            peer,
            from,
            channel,
            request,
            ok,
        } => {
            state.answered = state.answered.saturating_add(1);
            state.failed = state.failed.saturating_add(u64::from(!pow::publish(
                swarm, service, peer, &from, channel, &request, ok,
            )));
        }
        PowDone::Gossip {
            source,
            id,
            data,
            ok,
        } => {
            state.pow_gossip = state.pow_gossip.saturating_sub(1);
            pow::gossip(swarm, service, source, &id, &data, ok);
        }
        PowDone::CatchUp { records } => pow::catch_up(service, &records),
    }
}
