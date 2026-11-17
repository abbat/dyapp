//! The swarm event loop of a node.

use super::*;
mod catch_up;
mod events;
mod hangup;
mod overload;
mod pow;
mod requests;
mod search_jobs;
mod state;
mod stored;
use pow::PowDone;
use search_jobs::start_search;

fn clock_instant(service: &Service) -> tokio::time::Instant {
    tokio::time::Instant::from_std(service.clock.instant())
}

fn run_catch_up(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    asks: &mut crate::catch_up::Asks,
    refill_round: &mut refill::Round,
) {
    if service.busy(SHED_MEDIA) {
        return;
    }
    if !asks.idle() {
        tracing::debug!("search catch-up still running, tick skipped");
        return;
    }
    let Some(gossip) = swarm.behaviour().gossip.as_ref() else {
        return;
    };
    let peers: Vec<PeerId> = gossip.all_peers().map(|(peer, _)| *peer).collect();
    if peers.is_empty() {
        return;
    }
    refill_round.start(service);
    for topic in (0..SEARCH_TOPICS).filter(|t| service.config.search.keeps(*t)) {
        let Ok(pick) = getrandom::u32() else {
            continue;
        };
        for peer in catch_up_peers(&peers, pick) {
            asks.topic(peer, topic);
        }
    }
    asks.send(swarm);
}

fn request_deny_lists(swarm: &mut Swarm<Behaviour>, service: &Service, routed: &HashSet<PeerId>) {
    if !service.config.network.accept_deny_lists {
        return;
    }
    let request = proto::NodeRequest {
        request: Some(proto::node_request::Request::DenyList(
            proto::DenyListRequest {},
        )),
    };
    for peer in routed {
        if swarm.is_connected(peer) {
            swarm.behaviour_mut().node.send_request(peer, request);
        }
    }
}

/// Serves requests on `swarm` until the task is dropped. The deny list is checked for changes
/// every 10 seconds and, on Unix, on SIGHUP.
pub async fn run(mut swarm: Swarm<Behaviour>, service: Arc<Service>) {
    let Some(mut state) = state::State::new(&mut swarm, &service) else {
        return;
    };
    loop {
        state.dispatch(&service);
        tokio::select! {
            event = swarm.select_next_some() => {
                events::handle(state.events(&mut swarm, &service), event);
            }
            _ = state.timers.put_check.tick() => {
                state.replicas.finish_puts(&mut swarm, &service, service.clock.instant());
                state.replicas.republish(&mut swarm, &service, service.clock.instant());
            }
            _ = state.timers.republish_scan.tick() => state.replicas.scan(&mut swarm, &service, service.clock.instant()),
            _ = state.timers.deny_check.tick() => reload_deny(&service, &state.deny, &mut state.deny_version),
            () = state.hangup.recv() => {
                state.deny_version = None;
                reload_deny(&service, &state.deny, &mut state.deny_version);
                state.report_state(&swarm);
            }
            _ = state.timers.rejoin.tick() => {
                if dyapp_p2p_net::known_peers(&mut swarm).is_empty() {
                    dyapp_p2p_net::join(&mut swarm, &state.seeds, &[]);
                }
            }
            _ = state.timers.cleanup.tick() => {
                state.replicas.repaired.clear();
                stored::expire(&service);
            }
            Some(Finished { peer: id, connection, nonce, done }) = state.channels.store_done.recv() => {
                state.store_queue.done(id);
                if let Some(nonce) = nonce.filter(|_| swarm.behaviour().endpoints.ip(connection).is_some()) {
                    state.nonces.insert(connection, nonce);
                }
                let Some(done) = done else {
                    state.failed += 1;
                    tracing::error!(peer = %id, "store request panicked");
                    continue;
                };
                if stored::reply(&mut swarm, &service, &mut state.replicas, &mut state.watchers, (id, connection), done) {
                    state.answered += 1;
                } else {
                    state.failed += 1;
                }
            }
            _ = state.timers.puzzle_tick.tick() => service.puzzle.tick(service.clock.instant()),
            Some(done) = state.channels.search_done.recv() => {
                state.searching -= 1;
                start_search(&mut state.search_queue, &mut state.searching, &service, &state.channels.search_done_tx);
                let response = match done.reply {
                    Ok(reply) if done.get && !service.charge_get(&done.from, reply.encoded_len()) => SearchResponse {
                        status: Status::RateLimited.into(),
                        retry_after: 1,
                        ..SearchResponse::default()
                    },
                    Ok(reply) => reply,
                    Err(error) => {
                        state.failed += 1;
                        tracing::error!(%error, "search failed");
                        continue;
                    }
                };
                state.answered += 1;
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm.behaviour_mut().search.send_response(done.channel, response);
            }
            Some(done) = state.channels.pow_done.recv() => match done {
                PowDone::Publish { peer, from, channel, request, ok } => {
                    state.answered += 1;
                    state.failed += u64::from(!pow::publish(
                        &mut swarm, &service, peer, &from, channel, &request, ok,
                    ));
                }
                PowDone::Gossip { source, id, data, ok } => {
                    state.pow_gossip -= 1;
                    pow::gossip(&mut swarm, &service, source, &id, &data, ok);
                }
                PowDone::CatchUp { records } => pow::catch_up(&service, &records),
            },
            () = tokio::time::sleep_until(state.timers.catch_up_at), if !state.catch_up_queue.is_empty() => {
                state.timers.catch_up_at = send_catch_up(&mut swarm, &service.rebuild_budget, &mut state.catch_up_queue, &mut state.catch_up_asked, clock_instant(&service))
                    .unwrap_or_else(|| clock_instant(&service) + Duration::from_secs(1));
            }
            // Hourly, each kept topic's digest from two random search peers finds what gossip missed.
            _ = state.timers.catch_up.tick(), if service.search.is_some() => {
                run_catch_up(&mut swarm, &service, &mut state.asks, &mut state.refill_round);
            }
            _ = state.timers.maintain.tick() => {
                service.maintain(state.maintenance.vacuum_pages);
                service.report();
                service.rebuild_budget.report();
                // ponytail: saved hourly, not on shutdown; a crash loses at most an hour of churn.
                let peers = dyapp_p2p_net::known_peers(&mut swarm);
                let storage = &service.config.storage;
                if !peers.is_empty() {
                    if let Err(error) = save_peers(&storage.peers_path(), &peers) {
                        tracing::error!(%error, "peer cache not saved");
                    }
                }
                if !state.anchors.is_empty() {
                    if let Err(error) = save_peers(&storage.anchors_path(), &state.anchors) {
                        tracing::error!(%error, "anchors not saved");
                    }
                }
                let routed: HashSet<PeerId> = peers
                    .iter()
                    .filter_map(|a| match a.iter().last() {
                        Some(Protocol::P2p(peer)) => Some(peer),
                        _ => None,
                    })
                    .collect();
                state.replicas
                    .trust
                    .retain(|peer| routed.contains(peer) || swarm.is_connected(peer));
                request_deny_lists(&mut swarm, &service, &routed);
                let group_refused = swarm.behaviour_mut().incoming_groups.as_mut()
                    .map_or(0, dyapp_p2p_net::IncomingGroupGuard::take_denied);
                if state.refused > 0 {
                    tracing::warn!(refused = state.refused, group_refused, "connections refused by connection, IP group or memory limits");
                    state.refused = 0;
                }
                status(&service, &swarm, peers.len(), state.answered, state.failed, state.foreign_peers, state.replicas.media_holders.1, state.replicas.cap.take_dropped());
                state.report_state(&swarm);
                (state.answered, state.failed, state.foreign_peers) = (0, 0, 0);
                if service.search.is_some()
                    && swarm.behaviour().gossip.as_ref().is_some_and(|g| g.all_peers().count() < 6)
                {
                    swarm.behaviour_mut().kad.get_providers(kad::RecordKey::new(&SEARCH_KEY));
                }
                let _ = swarm.behaviour_mut().search_kad.bootstrap();
                dyapp_p2p_net::join(&mut swarm, &state.seeds, &[]);
            }
        }
    }
}
