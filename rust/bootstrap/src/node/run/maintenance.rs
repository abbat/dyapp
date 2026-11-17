use super::hangup::Hangup;
use super::state::State;
use super::{
    catch_up_peers, clock_instant, save_peers, send_catch_up, status, Arc, Behaviour, HashSet,
    Multiaddr, PeerId, Service, Swarm, SEARCH_KEY, SEARCH_TOPICS, SHED_MEDIA,
};
use libp2p::kad;
use libp2p::multiaddr::Protocol;

pub(super) enum ReplicationEvent {
    Put,
    Scan,
}

pub(super) async fn replication_event(
    put_check: &mut tokio::time::Interval,
    republish_scan: &mut tokio::time::Interval,
) -> ReplicationEvent {
    tokio::select! {
        _ = put_check.tick() => ReplicationEvent::Put,
        _ = republish_scan.tick() => ReplicationEvent::Scan,
    }
}

pub(super) fn replication(
    state: &mut State,
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    event: &ReplicationEvent,
) {
    match event {
        ReplicationEvent::Put => {
            state
                .replicas
                .finish_puts(swarm, service, service.clock.instant());
            state
                .replicas
                .republish(swarm, service, service.clock.instant());
        }
        ReplicationEvent::Scan => state.replicas.scan(swarm, service, service.clock.instant()),
    }
}

pub(super) enum DenyEvent {
    Tick,
    Hangup,
}

pub(super) async fn deny_event(
    deny_check: &mut tokio::time::Interval,
    hangup: &mut Hangup,
) -> DenyEvent {
    tokio::select! {
        _ = deny_check.tick() => DenyEvent::Tick,
        () = hangup.recv() => DenyEvent::Hangup,
    }
}

pub(super) fn deny(
    state: &mut State,
    swarm: &Swarm<Behaviour>,
    service: &Service,
    event: &DenyEvent,
) {
    let hangup = matches!(event, DenyEvent::Hangup);
    if hangup {
        state.deny_version = None;
    }
    super::reload_deny(service, &state.deny, &mut state.deny_version);
    if hangup {
        state.report_state(swarm);
    }
}

pub(super) fn rejoin(state: &State, swarm: &mut Swarm<Behaviour>) {
    if dyapp_p2p_net::known_peers(swarm).is_empty() {
        dyapp_p2p_net::join(swarm, &state.seeds, &[]);
    }
}

pub(super) fn send_queued(state: &mut State, swarm: &mut Swarm<Behaviour>, service: &Service) {
    state.timers.catch_up_at = send_catch_up(
        swarm,
        &service.rebuild_budget,
        &mut state.catch_up_queue,
        &mut state.catch_up_asked,
        clock_instant(service),
    )
    .unwrap_or_else(|| {
        let now = clock_instant(service);
        now.checked_add(std::time::Duration::from_secs(1))
            .unwrap_or(now)
    });
}

pub(super) fn catch_up(state: &mut State, swarm: &mut Swarm<Behaviour>, service: &Service) {
    if service.busy(SHED_MEDIA) {
        return;
    }
    if !state.asks.idle() {
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
    state.refill_round.start(service);
    for topic in (0..SEARCH_TOPICS).filter(|topic| service.config.search.keeps(*topic)) {
        let Ok(pick) = getrandom::u32() else {
            continue;
        };
        for peer in catch_up_peers(&peers, pick) {
            state.asks.topic(peer, topic);
        }
    }
    state.asks.send(swarm);
}

pub(super) fn tick(state: &mut State, swarm: &mut Swarm<Behaviour>, service: &Arc<Service>) {
    service.maintain(state.maintenance.vacuum_pages);
    service.report();
    service.rebuild_budget.report();
    let peers = dyapp_p2p_net::known_peers(swarm);
    save(
        &service.config.storage.peers_path(),
        &peers,
        "peer cache not saved",
    );
    save(
        &service.config.storage.anchors_path(),
        &state.anchors,
        "anchors not saved",
    );
    let routed = routed(&peers);
    state
        .replicas
        .trust
        .retain(|peer| routed.contains(peer) || swarm.is_connected(peer));
    request_deny_lists(swarm, service, &routed);
    report_refused(state, swarm);
    status(
        service,
        swarm,
        peers.len(),
        state.answered,
        state.failed,
        state.foreign_peers,
        state.replicas.media_holders.1,
        state.replicas.cap.take_dropped(),
    );
    state.report_state(swarm);
    (state.answered, state.failed, state.foreign_peers) = (0, 0, 0);
    search_providers(swarm, service);
    let _ = swarm.behaviour_mut().search_kad.bootstrap();
    dyapp_p2p_net::join(swarm, &state.seeds, &[]);
}

fn save(path: &std::path::Path, peers: &[Multiaddr], message: &str) {
    if peers.is_empty() {
        return;
    }
    // ponytail: saved hourly, not on shutdown; a crash loses at most an hour of churn.
    if let Err(error) = save_peers(path, peers) {
        tracing::error!(%error, "{message}");
    }
}

fn routed(peers: &[Multiaddr]) -> HashSet<PeerId> {
    peers
        .iter()
        .filter_map(|address| match address.iter().last() {
            Some(Protocol::P2p(peer)) => Some(peer),
            _ => None,
        })
        .collect()
}

fn request_deny_lists(swarm: &mut Swarm<Behaviour>, service: &Service, routed: &HashSet<PeerId>) {
    if !service.config.network.accept_deny_lists {
        return;
    }
    let request = dyapp_p2p_net::proto::NodeRequest {
        request: Some(dyapp_p2p_net::proto::node_request::Request::DenyList(
            dyapp_p2p_net::proto::DenyListRequest {},
        )),
    };
    for peer in routed {
        if swarm.is_connected(peer) {
            swarm.behaviour_mut().node.send_request(peer, request);
        }
    }
}

fn report_refused(state: &mut State, swarm: &mut Swarm<Behaviour>) {
    let group_refused = swarm
        .behaviour_mut()
        .incoming_groups
        .as_mut()
        .map_or(0, dyapp_p2p_net::IncomingGroupGuard::take_denied);
    if state.refused > 0 {
        tracing::warn!(
            refused = state.refused,
            group_refused,
            "connections refused by connection, IP group or memory limits"
        );
        state.refused = 0;
    }
}

fn search_providers(swarm: &mut Swarm<Behaviour>, service: &Service) {
    if service.search.is_some()
        && swarm
            .behaviour()
            .gossip
            .as_ref()
            .is_some_and(|gossip| gossip.all_peers().count() < 6)
    {
        swarm
            .behaviour_mut()
            .kad
            .get_providers(kad::RecordKey::new(&SEARCH_KEY));
    }
}
