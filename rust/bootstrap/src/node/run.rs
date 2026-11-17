//! The swarm event loop of a node.

use super::*;
mod catch_up;
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
            event = swarm.select_next_some() => {
            dyapp_p2p_net::route(&mut swarm, &event);
            service.metrics.observe(&swarm, &event);
            match event {
                SwarmEvent::NewListenAddr { address, .. } => tracing::info!(%address, "listening"),
                SwarmEvent::ExternalAddrConfirmed { address } => {
                    tracing::info!(%address, "external address confirmed");
                }
                // At max_memory_mb a request is dropped unanswered: its stream is reset and the
                // client tries another node.
                SwarmEvent::Behaviour(event) if service.overloaded() && overload::is_request(&event) => {}
                SwarmEvent::Behaviour(BehaviourEvent::Node(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    // Node requests touch no store, so they stay on the loop.
                    let response = service.node(&peer(&swarm, &service, id, connection_id), request);
                    answered += 1;
                    service.traffic.add(response.encoded_len() as u64);
                    let _ = swarm.behaviour_mut().node.send_response(channel, response);
                    drop_banned(&mut swarm, &service, id);
                }
                // The node asks other nodes only for deny lists.
                SwarmEvent::Behaviour(BehaviourEvent::Node(Event::Message {
                    peer: id,
                    message: Message::Response { response, .. },
                    ..
                })) => {
                    if let Some(record) = response.deny_list {
                        keep_deny_list(&deny, &id, &record, service.clock.timestamp());
                    }
                }
                SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => requests::profile(
                    &mut swarm,
                    &service,
                    &mut store_queue,
                    &pow_permits,
                    (id, connection_id),
                    request,
                    channel,
                ),
                SwarmEvent::Behaviour(BehaviourEvent::KeyPackage(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => requests::keypackage(
                    &mut swarm,
                    &service,
                    &mut store_queue,
                    id,
                    connection_id,
                    request,
                    channel,
                ),
                SwarmEvent::Behaviour(BehaviourEvent::Media(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    answered += u64::from(requests::media(
                        &mut swarm,
                        &service,
                        &mut store_queue,
                        &replicas,
                        (id, connection_id),
                        request,
                        channel,
                    ));
                }
                SwarmEvent::Behaviour(BehaviourEvent::Mailbox(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => requests::mailbox(
                    &mut swarm,
                    &service,
                    &mut store_queue,
                    id,
                    connection_id,
                    request,
                    channel,
                ),
                SwarmEvent::Behaviour(BehaviourEvent::Search(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    let outcome = search_jobs::request(
                        search_jobs::RequestContext {
                            swarm: &mut swarm,
                            service: &service,
                            queue: &mut search_queue,
                            running: &mut searching,
                            done: &search_done_tx,
                            pow_permits: &pow_permits,
                            pow_done: &pow_tx,
                        },
                        id,
                        connection_id,
                        request,
                        channel,
                    );
                    answered += outcome.answered;
                    failed += outcome.failed;
                }
                SwarmEvent::Behaviour(BehaviourEvent::Inventory(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => requests::inventory(
                    &mut swarm,
                    &service,
                    &mut store_queue,
                    id,
                    connection_id,
                    request,
                    channel,
                ),
                SwarmEvent::Behaviour(BehaviourEvent::Digest(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => requests::digest(
                    &mut swarm,
                    &service,
                    &mut store_queue,
                    id,
                    connection_id,
                    request,
                    channel,
                ),
                SwarmEvent::Behaviour(BehaviourEvent::Gossip(gossipsub::Event::Message {
                    propagation_source,
                    message_id,
                    message,
                })) => pow::receive_gossip(
                    &mut swarm,
                    &service,
                    &pow_permits,
                    &pow_tx,
                    &mut pow_gossip,
                    (propagation_source, message_id, message.data),
                ),
                // Catch-up: queue the keys this node lacks or holds older, and fetch the next page
                // while pages are full.
                SwarmEvent::Behaviour(BehaviourEvent::Inventory(Event::Message {
                    peer: id,
                    message: Message::Response { request_id, response },
                    ..
                })) => catch_up::inventory_response(
                    &mut swarm,
                    &service,
                    catch_up::InventoryState {
                        asks: &mut asks,
                        queue: &mut catch_up_queue,
                        at: &mut catch_up_at,
                        refill: &mut refill_round,
                    },
                    id,
                    request_id,
                    &response,
                ),
                // A catch-up get: apply what came, queue again what a reply cut at 1 MiB left out.
                // A reply with no records ends the chunk, so keys the peer no longer has drop out.
                SwarmEvent::Behaviour(BehaviourEvent::Search(Event::Message {
                    message: Message::Response { request_id, response },
                    ..
                })) => catch_up::search_response(
                    &mut swarm,
                    &service,
                    catch_up::SearchState {
                        queue: &mut catch_up_queue,
                        asked: &mut catch_up_asked,
                        at: &mut catch_up_at,
                    },
                    &pow_permits,
                    &pow_tx,
                    request_id,
                    &response,
                ),
                SwarmEvent::Behaviour(BehaviourEvent::Search(Event::OutboundFailure {
                    peer: id, request_id, ..
                })) => catch_up::search_failure(
                    &mut swarm,
                    &mut catch_up_queue,
                    &mut catch_up_asked,
                    id,
                    request_id,
                ),
                SwarmEvent::Behaviour(BehaviourEvent::Inventory(Event::OutboundFailure {
                    request_id, ..
                })) => catch_up::inventory_failure(&mut swarm, &mut asks, request_id),
                // Catch-up digests: ask what differs, or on failure the range the digest covered.
                SwarmEvent::Behaviour(BehaviourEvent::Digest(event)) => catch_up::digest(
                    &mut swarm,
                    &service,
                    &mut asks,
                    &mut refill_round,
                    event,
                ),
                SwarmEvent::ConnectionEstablished { peer_id, connection_id, endpoint, .. } => {
                    replicas.trust.seen.entry(peer_id).or_insert_with(|| service.clock.instant());
                    admission.established(&mut swarm, connection_id, peer_id, &endpoint, service.clock.instant());
                    drop_banned(&mut swarm, &service, peer_id);
                }
                SwarmEvent::ConnectionClosed { peer_id, connection_id, num_established, .. } => {
                    if num_established == 0 {
                        catch_up_queue.remove(&peer_id);
                        asks.forget(&peer_id);
                        service.search_peer(&peer_id.to_string(), false);
                        if !dyapp_p2p_net::routed(&mut swarm, &peer_id) {
                            replicas.trust.forget(&peer_id);
                        }
                    }
                    nonces.remove(&connection_id);
                    admission.closed(connection_id, peer_id, num_established == 0);
                    // ponytail: a scan of all watchers per closed connection; index them by
                    // connection if many devices watch at once.
                    watchers.retain(|_, list| {
                        list.retain(|(_, c)| *c != connection_id);
                        !list.is_empty()
                    });
                }
                SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
                    id,
                    result: kad::QueryResult::StartProviding(result),
                    ..
                })) if turn_query == Some(id) => {
                    turn_query = None;
                    match result {
                        Ok(_) => {
                            turn_unannounced = false;
                            tracing::info!("TURN relay announced");
                        }
                        Err(error) => tracing::warn!(%error, "TURN relay not announced"),
                    }
                }
                SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
                    id,
                    result: kad::QueryResult::StartProviding(result),
                    ..
                })) if search_query == Some(id) => {
                    search_query = None;
                    match result {
                        Ok(_) => {
                            search_unannounced = false;
                            tracing::info!("search node announced");
                        }
                        Err(error) => tracing::warn!(%error, "search node not announced"),
                    }
                }
                SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::InboundRequest {
                    request: kad::InboundRequest::AddProvider { record: Some(record) },
                })) => admission.add_provider(&mut swarm, record),
                // Only a search node with too few gossip peers looks for providers: dial them.
                SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
                    result: kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { providers, .. })),
                    ..
                })) => {
                    for provider in providers {
                        if provider != *swarm.local_peer_id()
                            && !swarm.is_connected(&provider)
                            && dyapp_p2p_net::id_has_pow(&provider, dyapp_p2p_net::id_pow_bits())
                        {
                            let _ = swarm.dial(provider);
                        }
                    }
                }
                SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
                    id,
                    result: kad::QueryResult::GetClosestPeers(result),
                    ..
                })) => {
                    replicas.closest(&mut swarm, &service, &watchers, id, result);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
                    result: kad::QueryResult::Bootstrap(Ok(ok)),
                    ..
                })) if ok.num_remaining == 0 => replicas.republish.bootstrapped(service.clock.instant()),
                // The peer's answer to an inventory: it gets what it lacks.
                SwarmEvent::Behaviour(BehaviourEvent::Mailbox(Event::Message {
                    peer: id,
                    message: Message::Response { request_id, response },
                    ..
                })) => {
                    replicas.mailbox_response(&mut swarm, &service, id, request_id, response);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Mailbox(Event::OutboundFailure {
                    peer,
                    request_id,
                    error,
                    ..
                })) => {
                    replicas.mailbox_failure(peer, request_id, &error);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::Message {
                    peer: id,
                    message: Message::Response { request_id, response },
                    ..
                })) => {
                    replicas.profile_response(&mut swarm, &service, id, request_id, response);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::OutboundFailure {
                    peer, request_id, error, ..
                })) => {
                    replicas.profile_failure(peer, request_id, &error);
                }
                SwarmEvent::Behaviour(BehaviourEvent::KeyPackage(Event::Message {
                    peer: id, message: Message::Response { request_id, response }, ..
                })) => {
                    replicas.keypackage_response(&mut swarm, &service, id, request_id, &response);
                }
                SwarmEvent::Behaviour(BehaviourEvent::KeyPackage(Event::OutboundFailure {
                    peer, request_id, error, ..
                })) => {
                    replicas.keypackage_failure(peer, request_id, &error);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Media(Event::Message {
                    peer: id, message: Message::Response { request_id, response }, ..
                })) => {
                    replicas.media_response(&mut swarm, &service, id, request_id, response);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Media(Event::OutboundFailure {
                    peer, request_id, error, ..
                })) => {
                    replicas.media_failure(&mut swarm, &service, peer, request_id, &error);
                }
                // Before any routing: a peer of another network enters no DHT and no anchors.
                SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                    connection_id, peer_id, info,
                })) if info.protocol_version != service.protocols.version => {
                    foreign_peers += 1;
                    identified::drop_foreign(&mut swarm, admission.dialed(connection_id), peer_id, &info.protocol_version);
                }
                // Kademlia learns a dialer's address only from identify: without this a node
                // never routes to peers that joined through it.
                SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                    connection_id, peer_id, info,
                })) if info.protocols.contains(&service.protocols.kad) || info.protocols.contains(&service.protocols.search_kad) => {
                    if !identified::add_node(&mut swarm, &service, &mut anchors, &mut admission, connection_id, peer_id, info) {
                        continue;
                    }
                    if turn_unannounced && turn_query.is_none() {
                        let key = kad::RecordKey::new(&dyapp_p2p_net::TURN_KEY);
                        turn_query = swarm.behaviour_mut().kad.start_providing(key).ok();
                    }
                    if search_unannounced && search_query.is_none() {
                        let key = kad::RecordKey::new(&SEARCH_KEY);
                        search_query = swarm.behaviour_mut().kad.start_providing(key).ok();
                    }
                }
                SwarmEvent::OutgoingConnectionError { connection_id, peer_id, error } => {
                    admission.failed(&mut swarm, connection_id, service.clock.instant());
                    if matches!(error, DialError::WrongPeerId { .. }) {
                        tracing::warn!(?peer_id, %error, "outbound peer ID mismatch; connection refused");
                    }
                }
                SwarmEvent::IncomingConnectionError { error: ListenError::Denied { .. }, .. } => {
                    refused += 1;
                }
                _ => {}
            }
            },
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
