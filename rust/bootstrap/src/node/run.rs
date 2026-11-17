//! The swarm event loop of a node.

use super::*;
mod catch_up;
mod events;
mod hangup;
mod overload;
mod pow;
mod requests;
mod search_jobs;
mod stored;
use pow::PowDone;
use search_jobs::{start_search, SearchDone, SearchJob};

fn clock_instant(service: &Service) -> tokio::time::Instant {
    tokio::time::Instant::from_std(service.clock.instant())
}

fn catch_up_timer(service: &Service) -> (tokio::time::Instant, tokio::time::Interval) {
    let now = clock_instant(service);
    (
        now,
        tokio::time::interval_at(now + Duration::from_secs(60), Duration::from_secs(3600)),
    )
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

fn store_local_providers(swarm: &mut Swarm<Behaviour>, turn: bool, search: bool) {
    // Until announced, the node lists itself only in its own store, enough for a client that asks it.
    let local = *swarm.local_peer_id();
    let keys = [(turn, dyapp_p2p_net::TURN_KEY), (search, SEARCH_KEY)];
    for (_, key) in keys.into_iter().filter(|(provides, _)| *provides) {
        use libp2p::kad::store::RecordStore as _;
        let record = kad::ProviderRecord::new(kad::RecordKey::new(&key), local, Vec::new());
        if let Err(error) = swarm.behaviour_mut().kad.store_mut().add_provider(record) {
            tracing::error!(%error, "provider record not stored");
        }
    }
}

fn subscribe_search_topics(swarm: &mut Swarm<Behaviour>, service: &Service) {
    if let Some(gossip) = swarm.behaviour_mut().gossip.as_mut() {
        for topic in (0..SEARCH_TOPICS).filter(|topic| service.config.search.keeps(*topic)) {
            if let Err(error) = gossip.subscribe(&service.protocols.search_topic(topic)) {
                tracing::error!(%error, topic, "search topic not joined");
            }
        }
    }
}

/// Serves requests on `swarm` until the task is dropped. The deny list is checked for changes
/// every 10 seconds and, on Unix, on SIGHUP.
pub async fn run(mut swarm: Swarm<Behaviour>, service: Arc<Service>) {
    let mut hangup = match hangup::Hangup::new() {
        Ok(hangup) => hangup,
        Err(error) => return tracing::error!(%error, "SIGHUP handler not installed"),
    };
    let storage = &service.config.storage;
    let mut anchors = cached_peers(&storage.anchors_path());
    let deny = match DenyStore::open(&storage.deny_path()) {
        Ok(deny) => deny,
        Err(error) => return tracing::error!(%error, "deny list not opened"),
    };
    let mut deny_version = None;
    reload_deny(&service, &deny, &mut deny_version);
    let mut deny_check = tokio::time::interval(Duration::from_secs(10));
    // The mailbox challenge issued on each open connection.
    let mut nonces: HashMap<ConnectionId, [u8; 32]> = HashMap::new();
    // The remote address of each connection, and identify addresses waiting for a probe.
    let mut admission = identified::Admission::default();
    let mut watchers = Watchers::new();
    let delay = u64::from(service.config.network.storage_trust_minutes) * 60;
    let mut replicas = Replicas::default();
    replicas.trust.delay = Duration::from_secs(delay);
    // Incoming connections refused by connection, group or memory limits since maintenance.
    let mut refused = 0u64;
    // A turn node announces its relay once it has a routing peer; kad republishes it every 12 h.
    // ponytail: every relay is a provider of one key, and a node keeps at most 20 providers per
    // key; shard the key (by region or prefix) when the network has more relays than that.
    let mut turn_unannounced = service.config.roles.contains(&Role::Turn);
    let mut turn_query = None;
    // A search node announces itself the same way, under SEARCH_KEY.
    let mut search_unannounced = service.search.is_some();
    let mut search_query = None;
    store_local_providers(&mut swarm, turn_unannounced, search_unannounced);
    subscribe_search_topics(&mut swarm, &service);
    // Index reads run off the swarm loop on two workers, fed from a queue fair between IP
    // groups; a read the queue refuses gets RATE_LIMITED. At most two items are ever in the
    // channel: one per worker.
    let mut search_queue = jobs::GroupQueue::<SearchJob>::default();
    let mut searching = 0;
    let (search_done_tx, mut search_done) = tokio::sync::mpsc::unbounded_channel::<SearchDone>();
    // Profile and heartbeat proof-of-work checks run off the loop on their own permits (half
    // the cores, at least two), so a flood of them cannot starve queries. A client publish without a permit gets RATE_LIMITED; gossip
    // and catch-up wait, gossip at most POW_GOSSIP_QUEUE messages.
    let pow_permits = Arc::new(tokio::sync::Semaphore::new(
        std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(2)),
    ));
    let (pow_tx, mut pow_done) = tokio::sync::mpsc::unbounded_channel::<PowDone>();
    let mut pow_gossip = 0usize;
    // Search catch-up: digests and inventory ranges waiting and in flight, the keys still to
    // fetch per peer, the keys of each get in flight, and when the next get may go out.
    let mut asks = crate::catch_up::Asks::default();
    let mut catch_up_queue: HashMap<PeerId, VecDeque<[u8; 32]>> = HashMap::new();
    let mut catch_up_asked: HashMap<OutboundRequestId, (PeerId, Vec<[u8; 32]>)> = HashMap::new();
    let (mut catch_up_at, mut catch_up) = catch_up_timer(&service);
    let mut refill_round = refill::Round::default();
    refill::spawn_rebuild(&service);
    // Requests answered and failed with a node error since the last maintenance run.
    // Connections dropped after identify because the peer belongs to another network.
    let (mut answered, mut failed, mut foreign_peers) = (0u64, 0u64, 0u64);
    let mut puzzle_tick = tokio::time::interval(crate::puzzle::TICK);
    let mut put_check = tokio::time::interval(Duration::from_millis(100));
    let mut cleanup = tokio::time::interval(Duration::from_secs(3600));
    let mut republish_scan = tokio::time::interval(republish::REPUBLISH_INTERVAL);
    let maintenance = service.config.maintenance.clone();
    let mut maintain = tokio::time::interval(Duration::from_secs(
        u64::from(maintenance.interval_minutes) * 60,
    ));
    // Seeds are dialled every maintenance run (the first at start) and, while the routing table
    // is empty, every 5 minutes. `NodeConfig::validate` has checked that they parse.
    let seeds: Vec<Multiaddr> = service
        .config
        .seeds
        .iter()
        .filter_map(|s| crate::config::seed(s).ok())
        .flatten()
        .collect();
    let every = Duration::from_secs(300);
    let mut rejoin = tokio::time::interval_at(clock_instant(&service) + every, every);
    // Store requests (node, profile, media, mailbox, inventory) run off the loop on
    // STORE_WORKERS threads, one per peer at a time, round robin between peers; past
    // PEER_QUEUE waiting the request is dropped. A peer's replies keep its request order.
    // ponytail: replication replies, gossip, repair, media inventory and maintenance still call
    // the store on the loop; move them here if their latency shows.
    let mut store_queue = jobs::FairQueue::<(ConnectionId, Job)>::default();
    let (store_tx, mut store_done) = tokio::sync::mpsc::unbounded_channel::<Finished>();
    loop {
        dispatch(&mut store_queue, &mut nonces, &service, &store_tx);
        if asks.idle()
            && catch_up_asked.is_empty()
            && catch_up_queue.values().all(VecDeque::is_empty)
        {
            refill_round.drained(&service);
        }
        tokio::select! {
            event = swarm.select_next_some() => events::handle(events::Context {
                swarm: &mut swarm,
                service: &service,
                anchors: &mut anchors,
                deny: &deny,
                nonces: &mut nonces,
                admission: &mut admission,
                watchers: &mut watchers,
                replicas: &mut replicas,
                refused: &mut refused,
                turn_unannounced: &mut turn_unannounced,
                turn_query: &mut turn_query,
                search_unannounced: &mut search_unannounced,
                search_query: &mut search_query,
                search_queue: &mut search_queue,
                searching: &mut searching,
                search_done: &search_done_tx,
                pow_permits: &pow_permits,
                pow_done: &pow_tx,
                pow_gossip: &mut pow_gossip,
                asks: &mut asks,
                catch_up_queue: &mut catch_up_queue,
                catch_up_asked: &mut catch_up_asked,
                catch_up_at: &mut catch_up_at,
                refill_round: &mut refill_round,
                answered: &mut answered,
                failed: &mut failed,
                foreign_peers: &mut foreign_peers,
                store_queue: &mut store_queue,
            }, event),
            _ = put_check.tick() => {
                replicas.finish_puts(&mut swarm, &service, service.clock.instant());
                replicas.republish(&mut swarm, &service, service.clock.instant());
            }
            _ = republish_scan.tick() => replicas.scan(&mut swarm, &service, service.clock.instant()),
            _ = deny_check.tick() => reload_deny(&service, &deny, &mut deny_version),
            () = hangup.recv() => {
                deny_version = None;
                reload_deny(&service, &deny, &mut deny_version);
                state(&replicas, &swarm, &[nonces.len(), swarm.behaviour().endpoints.len(), admission.len()], &watchers);
            }
            _ = rejoin.tick() => {
                if dyapp_p2p_net::known_peers(&mut swarm).is_empty() {
                    dyapp_p2p_net::join(&mut swarm, &seeds, &[]);
                }
            }
            _ = cleanup.tick() => {
                replicas.repaired.clear();
                stored::expire(&service);
            }
            Some(Finished { peer: id, connection, nonce, done }) = store_done.recv() => {
                store_queue.done(id);
                if let Some(nonce) = nonce.filter(|_| swarm.behaviour().endpoints.ip(connection).is_some()) {
                    nonces.insert(connection, nonce);
                }
                let Some(done) = done else {
                    failed += 1;
                    tracing::error!(peer = %id, "store request panicked");
                    continue;
                };
                if stored::reply(&mut swarm, &service, &mut replicas, &mut watchers, (id, connection), done) {
                    answered += 1;
                } else {
                    failed += 1;
                }
            }
            _ = puzzle_tick.tick() => service.puzzle.tick(service.clock.instant()),
            Some(done) = search_done.recv() => {
                searching -= 1;
                start_search(&mut search_queue, &mut searching, &service, &search_done_tx);
                let response = match done.reply {
                    Ok(reply) if done.get && !service.charge_get(&done.from, reply.encoded_len()) => SearchResponse {
                        status: Status::RateLimited.into(),
                        retry_after: 1,
                        ..SearchResponse::default()
                    },
                    Ok(reply) => reply,
                    Err(error) => {
                        failed += 1;
                        tracing::error!(%error, "search failed");
                        continue;
                    }
                };
                answered += 1;
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm.behaviour_mut().search.send_response(done.channel, response);
            }
            Some(done) = pow_done.recv() => match done {
                PowDone::Publish { peer, from, channel, request, ok } => {
                    answered += 1;
                    failed += u64::from(!pow::publish(
                        &mut swarm, &service, peer, &from, channel, &request, ok,
                    ));
                }
                PowDone::Gossip { source, id, data, ok } => {
                    pow_gossip -= 1;
                    pow::gossip(&mut swarm, &service, source, &id, &data, ok);
                }
                PowDone::CatchUp { records } => pow::catch_up(&service, &records),
            },
            () = tokio::time::sleep_until(catch_up_at), if !catch_up_queue.is_empty() => {
                catch_up_at = send_catch_up(&mut swarm, &service.rebuild_budget, &mut catch_up_queue, &mut catch_up_asked, clock_instant(&service))
                    .unwrap_or_else(|| clock_instant(&service) + Duration::from_secs(1));
            }
            // Hourly, each kept topic's digest from two random search peers finds what gossip missed.
            _ = catch_up.tick(), if service.search.is_some() => {
                run_catch_up(&mut swarm, &service, &mut asks, &mut refill_round);
            }
            _ = maintain.tick() => {
                service.maintain(maintenance.vacuum_pages);
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
                if !anchors.is_empty() {
                    if let Err(error) = save_peers(&storage.anchors_path(), &anchors) {
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
                replicas
                    .trust
                    .retain(|peer| routed.contains(peer) || swarm.is_connected(peer));
                request_deny_lists(&mut swarm, &service, &routed);
                let group_refused = swarm.behaviour_mut().incoming_groups.as_mut()
                    .map_or(0, dyapp_p2p_net::IncomingGroupGuard::take_denied);
                if refused > 0 {
                    tracing::warn!(refused, group_refused, "connections refused by connection, IP group or memory limits");
                    refused = 0;
                }
                status(&service, &swarm, peers.len(), answered, failed, foreign_peers, replicas.media_holders.1, replicas.cap.take_dropped());
                state(&replicas, &swarm, &[nonces.len(), swarm.behaviour().endpoints.len(), admission.len()], &watchers);
                (answered, failed, foreign_peers) = (0, 0, 0);
                if service.search.is_some()
                    && swarm.behaviour().gossip.as_ref().is_some_and(|g| g.all_peers().count() < 6)
                {
                    swarm.behaviour_mut().kad.get_providers(kad::RecordKey::new(&SEARCH_KEY));
                }
                let _ = swarm.behaviour_mut().search_kad.bootstrap();
                dyapp_p2p_net::join(&mut swarm, &seeds, &[]);
            }
        }
    }
}
