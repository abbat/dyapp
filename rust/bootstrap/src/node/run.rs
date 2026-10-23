//! The swarm event loop of a node.

use super::*;
mod overload;
mod search_jobs;
mod stored;
use search_jobs::{start_search, SearchDone, SearchJob};

/// Serves requests on `swarm` until the task is dropped. The deny list is checked for changes
/// every 10 seconds and on SIGHUP; an old `<storage.dir>/deny` file is imported once at start.
pub async fn run(mut swarm: Swarm<Behaviour>, service: Arc<Service>) {
    use tokio::signal::unix::{signal, SignalKind};
    let mut hangup = match signal(SignalKind::hangup()) {
        Ok(hangup) => hangup,
        Err(error) => return tracing::error!(%error, "SIGHUP handler not installed"),
    };
    let storage = &service.config.storage;
    let mut anchors = cached_peers(&storage.anchors_path());
    let deny = match DenyStore::open(&storage.deny_path()) {
        Ok(deny) => deny,
        Err(error) => return tracing::error!(%error, "deny list not opened"),
    };
    match deny.import(&storage.dir.join("deny"), &service.config.limits) {
        Ok(0) => {}
        Ok(imported) => tracing::info!(imported, "deny file moved into deny.db"),
        Err(error) => tracing::error!(%error, "deny file not imported"),
    }
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
    if let Some(gossip) = swarm.behaviour_mut().gossip.as_mut() {
        for topic in (0..SEARCH_TOPICS).filter(|t| service.config.search.keeps(*t)) {
            if let Err(error) = gossip.subscribe(&service.protocols.search_topic(topic)) {
                tracing::error!(%error, topic, "search topic not joined");
            }
        }
    }
    // Index reads run off the swarm loop on two workers, fed from a queue fair between IP
    // groups; a read the queue refuses gets RATE_LIMITED. At most two items are ever in the
    // channel: one per worker.
    let mut search_queue = jobs::GroupQueue::<SearchJob>::default();
    let mut searching = 0;
    let (search_done_tx, mut search_done) = tokio::sync::mpsc::unbounded_channel::<SearchDone>();
    // Profile and heartbeat proof-of-work checks run off the loop on their own permits (half
    // the cores, at least two), so a flood of them cannot starve queries. A client publish without a permit gets RATE_LIMITED; gossip
    // and catch-up wait, gossip at most POW_GOSSIP_QUEUE messages.
    enum PowDone {
        Publish {
            peer: PeerId,
            from: Peer,
            channel: ResponseChannel<SearchResponse>,
            request: proto::SearchRequest,
            ok: bool,
        },
        Gossip {
            source: PeerId,
            id: gossipsub::MessageId,
            data: Vec<u8>,
            ok: bool,
        },
        CatchUp {
            records: Vec<SignedRecord>,
        },
    }
    const POW_GOSSIP_QUEUE: usize = 64;
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
    let mut catch_up_at = tokio::time::Instant::now();
    let mut refill_round = refill::Round::default();
    refill::spawn_rebuild(&service);
    let mut catch_up = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(60),
        Duration::from_secs(3600),
    );
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
    let mut rejoin = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
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
                    tracing::info!(%address, "external address confirmed")
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
                        keep_deny_list(&deny, &id, &record);
                    }
                }
                SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    let from = peer(&swarm, &service, id, connection_id);
                    let permits = pow_permits.clone();
                    enqueue(&mut store_queue, id, connection_id, Box::new(move |service, _| {
                        let check_pow = |check: &crate::pow::PowCheck| {
                            let _permit = permits.clone().try_acquire_owned().ok()?;
                            Some(service.pow.check(check))
                        };
                        if let Some(proto::profile_request::Request::Publish(record)) = &request.request {
                            let result = service.prepare_profile_with_pow(&from, record.clone(), &check_pow);
                            return StoreDone::ProfilePublish(channel, record.clone(), result);
                        }
                        let heartbeat = match &request.request {
                            Some(proto::profile_request::Request::Heartbeat(record)) => Some(record.public_key.clone()),
                            _ => None,
                        };
                        let result = service.profile_with_pow(&from, request, &check_pow);
                        let ok = matches!(&result, Ok(response) if response.status == i32::from(Status::Ok));
                        // An accepted heartbeat repairs the stored profile's replicas.
                        let stored = heartbeat.filter(|_| ok).and_then(|key| {
                            let owner = dyapp_identity::key_hash(&key.as_slice().try_into().ok()?);
                            let owner = owner.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
                            service.store.get_profile(&owner).unwrap_or_else(|error| {
                                tracing::error!(%error, "heartbeat repair failed");
                                None
                            })
                        });
                        StoreDone::Profile(channel, result, stored)
                    }));
                }
                SwarmEvent::Behaviour(BehaviourEvent::KeyPackage(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    let from = peer(&swarm, &service, id, connection_id);
                    enqueue(&mut store_queue, id, connection_id, Box::new(move |service, _| {
                        if let Some(proto::key_package_request::Request::Publish(record)) = &request.request {
                            let result = service.prepare_keypackage(&from, record.clone());
                            return StoreDone::KeyPackagePublish(channel, record.clone(), result);
                        }
                        StoreDone::KeyPackage(channel, service.keypackage(&from, request))
                    }));
                }
                SwarmEvent::Behaviour(BehaviourEvent::Media(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    let put = matches!(request.request, Some(proto::media_request::Request::Put(_)));
                    if put && replicas.at_capacity() {
                        answered += 1;
                        let response = crate::service::media_upload_status(Status::Full);
                        service.traffic.add(response.encoded_len() as u64);
                        let _ = swarm.behaviour_mut().media.send_response(channel, response);
                        continue;
                    }
                    let from = peer(&swarm, &service, id, connection_id);
                    enqueue(&mut store_queue, id, connection_id, Box::new(move |service, _| {
                        if let Some(proto::media_request::Request::Put(put)) = &request.request {
                            return StoreDone::MediaPut(channel, service.prepare_media(&from, put));
                        }
                        let keep = match &request.request {
                            Some(proto::media_request::Request::Keep(record)) => Some(record.clone()),
                            _ => None,
                        };
                        let have = match &request.request {
                            Some(proto::media_request::Request::Have(have)) => Some(have.items.clone()),
                            _ => None,
                        };
                        let get = match &request.request {
                            Some(proto::media_request::Request::Get(get)) => Some(get.clone()),
                            _ => None,
                        };
                        let result = service.media(&from, request);
                        let missing = matches!(&result, Ok(response) if response.status == i32::from(Status::NotFound));
                        let manifest = match (get, &service.media) {
                            (Some(get), Some(media)) if missing => {
                                media.manifest(&get.hash).ok().flatten().map(|manifest| (manifest, get))
                            }
                            _ => None,
                        };
                        StoreDone::Media { channel, result, keep, have, manifest }
                    }));
                }
                SwarmEvent::Behaviour(BehaviourEvent::Mailbox(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    let from = peer(&swarm, &service, id, connection_id);
                    enqueue(&mut store_queue, id, connection_id, Box::new(move |service, nonce| {
                        if let Some(mailbox_request::Request::Put(record)) = &request.request {
                            let result = service.prepare_mailbox(&from, record.clone());
                            let group = service.put_group(record, &from);
                            return StoreDone::MailboxPut(channel, record.clone(), group, result);
                        }
                        let kind = request.request.clone();
                        StoreDone::Mailbox(channel, kind, service.mailbox(&from, nonce, request))
                    }));
                }
                SwarmEvent::Behaviour(BehaviourEvent::Search(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    let gossip = search_key(&request).map(|key| (key, request.encode_to_vec()));
                    let solution = match &request.request {
                        Some(proto::search_request::Request::Query(query)) => query.puzzle.clone(),
                        Some(proto::search_request::Request::Get(get)) => get.puzzle.clone(),
                        _ => None,
                    };
                    let from = peer(&swarm, &service, id, connection_id);
                    match service.search(&from, request) {
                        Ok(Search::Reply(response)) => {
                            answered += 1;
                            spread(&mut swarm, gossip, &response, &service);
                            service.traffic.add(response.encoded_len() as u64);
                            let _ = swarm.behaviour_mut().search.send_response(channel, response);
                        }
                        Ok(Search::Pow(check, request)) => match pow_permits.clone().try_acquire_owned() {
                            Ok(permit) => {
                                let (cache, done) = (service.pow.clone(), pow_tx.clone());
                                tokio::task::spawn_blocking(move || {
                                    let _permit = permit;
                                    let ok = cache.check(&check);
                                    let _ = done.send(PowDone::Publish { peer: id, from, channel, request, ok });
                                });
                            }
                            Err(_) => {
                                answered += 1;
                                let response = SearchResponse {
                                    status: Status::RateLimited.into(),
                                    retry_after: 1,
                                    ..SearchResponse::default()
                                };
                                let _ = swarm.behaviour_mut().search.send_response(channel, response);
                            }
                        },
                        Ok(work) => {
                            let group = (!service.is_search_peer(&from)).then(|| from.group.clone());
                            let peer_id = from.id.clone();
                            let job = SearchJob { from, channel, work, queued: std::time::Instant::now() };
                            match search_queue.push(group.as_deref(), job) {
                                Ok(()) => {
                                    if let Some(solution) = &solution {
                                        service.puzzle.consume(&peer_id, solution);
                                    }
                                }
                                Err(SearchJob { channel, .. }) => {
                                    answered += 1;
                                    let response = SearchResponse {
                                        status: Status::RateLimited.into(),
                                        retry_after: 1,
                                        ..SearchResponse::default()
                                    };
                                    let _ = swarm.behaviour_mut().search.send_response(channel, response);
                                }
                            }
                            start_search(&mut search_queue, &mut searching, &service, &search_done_tx);
                        }
                        Err(error) => {
                            failed += 1;
                            tracing::error!(%error, "search request failed");
                        }
                    }
                    drop_banned(&mut swarm, &service, id);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Inventory(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    let from = peer(&swarm, &service, id, connection_id);
                    enqueue(&mut store_queue, id, connection_id, Box::new(move |service, _| {
                        StoreDone::Inventory(channel, service.search_inventory(&from, request))
                    }));
                }
                SwarmEvent::Behaviour(BehaviourEvent::Digest(Event::Message {
                    peer: id,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    let from = peer(&swarm, &service, id, connection_id);
                    enqueue(&mut store_queue, id, connection_id, Box::new(move |service, _| {
                        StoreDone::Digest(channel, service.search_digest(&from, request))
                    }));
                }
                SwarmEvent::Behaviour(BehaviourEvent::Gossip(gossipsub::Event::Message {
                    propagation_source,
                    message_id,
                    message,
                })) => {
                    service.wire.add(dyapp_p2p_net::WireBytes::GOSSIP, dyapp_p2p_net::Dir::In, message.data.len());
                    match service.search_gossip(&message.data) {
                        // Queue full: drop without a strike.
                        Verdict::Pow(_) if pow_gossip >= POW_GOSSIP_QUEUE => {
                            judge(&mut swarm, &service, &message_id, &propagation_source, Verdict::Ignore);
                        }
                        Verdict::Pow(check) => {
                            pow_gossip += 1;
                            let (permits, cache, done) = (pow_permits.clone(), service.pow.clone(), pow_tx.clone());
                            let data = message.data;
                            tokio::spawn(async move {
                                let Ok(permit) = permits.acquire_owned().await else { return };
                                let _ = tokio::task::spawn_blocking(move || {
                                    let _permit = permit;
                                    let ok = cache.check(&check);
                                    let _ = done.send(PowDone::Gossip { source: propagation_source, id: message_id, data, ok });
                                })
                                .await;
                            });
                        }
                        verdict => judge(&mut swarm, &service, &message_id, &propagation_source, verdict),
                    }
                }
                // Catch-up: queue the keys this node lacks or holds older, and fetch the next page
                // while pages are full.
                SwarmEvent::Behaviour(BehaviourEvent::Inventory(Event::Message {
                    peer: id,
                    message: Message::Response { request_id, response },
                    ..
                })) => {
                    let (Some(range), Some(index)) = (asks.range(&request_id), &service.search) else {
                        continue;
                    };
                    refill_round.answered();
                    let page = response.have.iter().filter_map(|h| Some((h.key.as_slice().try_into().ok()?, h.version)));
                    let (have, next) = crate::catch_up::on_inventory(&range, page.collect());
                    let keys: Vec<[u8; 32]> = have.iter().map(|h| h.0).collect();
                    match index.versions(&keys) {
                        Ok(local) => {
                            let queue = catch_up_queue.entry(id).or_default();
                            // ponytail: O(n) contains per key; a page holds at most 10 000 keys.
                            for ((key, version), local) in have.iter().zip(local) {
                                if local.is_none_or(|l| l < *version) && !queue.contains(key) {
                                    queue.push_back(*key);
                                }
                            }
                            catch_up_at = catch_up_at.min(tokio::time::Instant::now());
                        }
                        Err(error) => tracing::error!(%error, "search catch-up failed"),
                    }
                    if let Some(next) = next {
                        asks.next_page(id, next);
                    }
                    asks.send(&mut swarm);
                }
                // A catch-up get: apply what came, queue again what a reply cut at 1 MiB left out.
                // A reply with no records ends the chunk, so keys the peer no longer has drop out.
                SwarmEvent::Behaviour(BehaviourEvent::Search(Event::Message {
                    message: Message::Response { request_id, response },
                    ..
                })) => {
                    let Some((id, keys)) = catch_up_asked.remove(&request_id) else { continue };
                    let mut got = HashSet::new();
                    let mut stored = 0;
                    let mut pending = Vec::new();
                    for record in &response.records {
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
                    // One job per reply checks the unproven records, then applies those that pass.
                    if !pending.is_empty() {
                        let (permits, cache, done) = (pow_permits.clone(), service.pow.clone(), pow_tx.clone());
                        tokio::spawn(async move {
                            let Ok(permit) = permits.acquire_owned().await else { return };
                            let _ = tokio::task::spawn_blocking(move || {
                                let _permit = permit;
                                let records = pending.into_iter().filter(|(c, _)| cache.check(c)).map(|(_, r)| r).collect();
                                let _ = done.send(PowDone::CatchUp { records });
                            })
                            .await;
                        });
                    }
                    tracing::debug!(stored, "search catch-up records applied");
                    if !got.is_empty() {
                        catch_up_queue.entry(id).or_default().extend(keys.into_iter().filter(|k| !got.contains(k)));
                    }
                    if let Some(next) = send_catch_up(&mut swarm, &service.rebuild_budget, &mut catch_up_queue, &mut catch_up_asked) {
                        catch_up_at = next;
                    }
                }
                SwarmEvent::Behaviour(BehaviourEvent::Search(Event::OutboundFailure {
                    peer: id, request_id, ..
                })) => {
                    if let Some((_, keys)) = catch_up_asked.remove(&request_id) {
                        if swarm.is_connected(&id) {
                            catch_up_queue.entry(id).or_default().extend(keys);
                        }
                    }
                }
                SwarmEvent::Behaviour(BehaviourEvent::Inventory(Event::OutboundFailure {
                    request_id, ..
                })) => {
                    asks.range(&request_id);
                    asks.send(&mut swarm);
                }
                // Catch-up digests: ask what differs, or on failure the range the digest covered.
                SwarmEvent::Behaviour(BehaviourEvent::Digest(event)) => {
                    let Some(index) = &service.search else { continue };
                    if asks.digest(&mut swarm, index, event) {
                        refill_round.answered();
                    }
                }
                SwarmEvent::ConnectionEstablished { peer_id, connection_id, endpoint, .. } => {
                    replicas.trust.seen.entry(peer_id).or_insert_with(Instant::now);
                    admission.established(&mut swarm, connection_id, peer_id, &endpoint);
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
                })) if ok.num_remaining == 0 => replicas.republish.bootstrapped(Instant::now()),
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
                    replicas.keypackage_response(&mut swarm, &service, id, request_id, response);
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
                    identified::drop_foreign(&mut swarm, admission.dialed(&connection_id), peer_id, &info.protocol_version);
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
                    admission.failed(&mut swarm, connection_id);
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
                replicas.finish_puts(&mut swarm, &service, Instant::now());
                replicas.republish(&mut swarm, &service, Instant::now());
            }
            _ = republish_scan.tick() => replicas.scan(&mut swarm, &service, Instant::now()),
            _ = deny_check.tick() => reload_deny(&service, &deny, &mut deny_version),
            _ = hangup.recv() => {
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
            _ = puzzle_tick.tick() => service.puzzle.tick(std::time::Instant::now()),
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
                    match service.search_pow_done(&from, &request, ok) {
                        Ok(response) => {
                            let gossip = search_key(&request).map(|key| (key, request.encode_to_vec()));
                            spread(&mut swarm, gossip, &response, &service);
                            service.traffic.add(response.encoded_len() as u64);
                            let _ = swarm.behaviour_mut().search.send_response(channel, response);
                        }
                        Err(error) => {
                            failed += 1;
                            tracing::error!(%error, "search request failed");
                        }
                    }
                    drop_banned(&mut swarm, &service, peer);
                }
                PowDone::Gossip { source, id, data, ok } => {
                    pow_gossip -= 1;
                    let verdict = match ok.then(|| service.search_gossip(&data)) {
                        None => Verdict::Reject,
                        // A cache cleared since the check asks again: drop without a strike.
                        Some(Verdict::Pow(_)) => Verdict::Ignore,
                        Some(verdict) => verdict,
                    };
                    judge(&mut swarm, &service, &id, &source, verdict);
                }
                PowDone::CatchUp { records } => {
                    let stored = records.iter().filter(|r| matches!(service.search_apply(r), Ok(Caught::Stored))).count();
                    tracing::debug!(stored, "search catch-up records applied after proof of work");
                }
            },
            _ = tokio::time::sleep_until(catch_up_at), if !catch_up_queue.is_empty() => {
                catch_up_at = send_catch_up(&mut swarm, &service.rebuild_budget, &mut catch_up_queue, &mut catch_up_asked)
                    .unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(1));
            }
            // Hourly, each kept topic's digest from two random search peers finds what gossip missed.
            _ = catch_up.tick(), if service.search.is_some() => {
                if service.busy(SHED_MEDIA) {
                    continue;
                }
                if !asks.idle() {
                    tracing::debug!("search catch-up still running, tick skipped");
                    continue;
                }
                let Some(gossip) = swarm.behaviour().gossip.as_ref() else { continue };
                let peers: Vec<PeerId> = gossip.all_peers().map(|(peer, _)| *peer).collect();
                if peers.is_empty() {
                    continue;
                }
                refill_round.start(&service);
                for topic in (0..SEARCH_TOPICS).filter(|t| service.config.search.keeps(*t)) {
                    let Ok(pick) = getrandom::u32() else { continue };
                    for peer in catch_up_peers(&peers, pick) {
                        asks.topic(peer, topic);
                    }
                }
                asks.send(&mut swarm);
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
                if service.config.network.accept_deny_lists {
                    let request = proto::NodeRequest {
                        request: Some(proto::node_request::Request::DenyList(
                            proto::DenyListRequest {},
                        )),
                    };
                    for peer in &routed {
                        if swarm.is_connected(peer) {
                            swarm.behaviour_mut().node.send_request(peer, request);
                        }
                    }
                }
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
