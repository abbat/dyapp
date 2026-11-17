use super::pow::PowDone;
use super::search_jobs::{SearchDone, SearchJob};
use super::*;

pub(super) struct Context<'a> {
    pub(super) swarm: &'a mut Swarm<Behaviour>,
    pub(super) service: &'a Arc<Service>,
    pub(super) anchors: &'a mut Vec<Multiaddr>,
    pub(super) deny: &'a DenyStore,
    pub(super) nonces: &'a mut HashMap<ConnectionId, [u8; 32]>,
    pub(super) admission: &'a mut identified::Admission,
    pub(super) watchers: &'a mut Watchers,
    pub(super) replicas: &'a mut Replicas,
    pub(super) refused: &'a mut u64,
    pub(super) turn_unannounced: &'a mut bool,
    pub(super) turn_query: &'a mut Option<kad::QueryId>,
    pub(super) search_unannounced: &'a mut bool,
    pub(super) search_query: &'a mut Option<kad::QueryId>,
    pub(super) search_queue: &'a mut jobs::GroupQueue<SearchJob>,
    pub(super) searching: &'a mut usize,
    pub(super) search_done: &'a tokio::sync::mpsc::UnboundedSender<SearchDone>,
    pub(super) pow_permits: &'a Arc<tokio::sync::Semaphore>,
    pub(super) pow_done: &'a tokio::sync::mpsc::UnboundedSender<PowDone>,
    pub(super) pow_gossip: &'a mut usize,
    pub(super) asks: &'a mut crate::catch_up::Asks,
    pub(super) catch_up_queue: &'a mut HashMap<PeerId, VecDeque<[u8; 32]>>,
    pub(super) catch_up_asked: &'a mut HashMap<OutboundRequestId, (PeerId, Vec<[u8; 32]>)>,
    pub(super) catch_up_at: &'a mut tokio::time::Instant,
    pub(super) refill_round: &'a mut refill::Round,
    pub(super) answered: &'a mut u64,
    pub(super) failed: &'a mut u64,
    pub(super) foreign_peers: &'a mut u64,
    pub(super) store_queue: &'a mut jobs::FairQueue<(ConnectionId, Job)>,
}

pub(super) fn handle(mut context: Context<'_>, event: SwarmEvent<BehaviourEvent>) {
    dyapp_p2p_net::route(context.swarm, &event);
    context.service.metrics.observe(context.swarm, &event);
    match event {
        SwarmEvent::Behaviour(event) => behaviour(&mut context, event),
        SwarmEvent::NewListenAddr { address, .. } => tracing::info!(%address, "listening"),
        SwarmEvent::ExternalAddrConfirmed { address } => external_addr(&address),
        SwarmEvent::ConnectionEstablished {
            peer_id,
            connection_id,
            endpoint,
            ..
        } => established(&mut context, peer_id, connection_id, &endpoint),
        SwarmEvent::ConnectionClosed {
            peer_id,
            connection_id,
            num_established,
            ..
        } => closed(&mut context, peer_id, connection_id, num_established),
        SwarmEvent::OutgoingConnectionError {
            connection_id,
            peer_id,
            error,
        } => outgoing_error(&mut context, connection_id, peer_id, &error),
        SwarmEvent::IncomingConnectionError {
            error: ListenError::Denied { .. },
            ..
        } => *context.refused = (*context.refused).saturating_add(1),
        _ => {}
    }
}

fn external_addr(address: &Multiaddr) {
    tracing::info!(%address, "external address confirmed");
}

fn outgoing_error(
    context: &mut Context<'_>,
    connection_id: ConnectionId,
    peer_id: Option<PeerId>,
    error: &DialError,
) {
    context.admission.failed(
        context.swarm,
        connection_id,
        context.service.clock.instant(),
    );
    if matches!(error, DialError::WrongPeerId { .. }) {
        tracing::warn!(?peer_id, %error, "outbound peer ID mismatch; connection refused");
    }
}

fn behaviour(context: &mut Context<'_>, event: BehaviourEvent) {
    // At max_memory_mb a request is dropped unanswered: its stream is reset and the
    // client tries another node.
    if context.service.overloaded() && overload::is_request(&event) {
        return;
    }
    match event {
        BehaviourEvent::Node(event) => node(context, event),
        BehaviourEvent::Profile(event) => profile(context, event),
        BehaviourEvent::KeyPackage(event) => keypackage(context, event),
        BehaviourEvent::Media(event) => media(context, event),
        BehaviourEvent::Mailbox(event) => mailbox(context, event),
        BehaviourEvent::Search(event) => search(context, event),
        BehaviourEvent::Inventory(event) => inventory(context, event),
        BehaviourEvent::Digest(event) => digest(context, event),
        BehaviourEvent::Gossip(event) => gossip(context, event),
        BehaviourEvent::Kad(event) => kad_event(context, event),
        BehaviourEvent::Identify(event) => identified(context, event),
        _ => {}
    }
}

fn node(context: &mut Context<'_>, event: Event<proto::NodeRequest, proto::NodeResponse>) {
    match event {
        Event::Message {
            peer: id,
            connection_id,
            message: Message::Request {
                request, channel, ..
            },
        } => {
            // Node requests touch no store, so they stay on the loop.
            let from = peer(context.swarm, context.service, id, connection_id);
            let response = context.service.node(&from, request);
            *context.answered = (*context.answered).saturating_add(1);
            context.service.traffic.add(response.encoded_len() as u64);
            let _ = context
                .swarm
                .behaviour_mut()
                .node
                .send_response(channel, response);
            drop_banned(context.swarm, context.service, id);
        }
        // The node asks other nodes only for deny lists.
        Event::Message {
            peer: id,
            message: Message::Response { response, .. },
            ..
        } => {
            if let Some(record) = response.deny_list {
                keep_deny_list(
                    context.deny,
                    &id,
                    &record,
                    context.service.clock.timestamp(),
                );
            }
        }
        _ => {}
    }
}

fn profile(context: &mut Context<'_>, event: Event<proto::ProfileRequest, proto::ProfileResponse>) {
    match event {
        Event::Message {
            peer: id,
            connection_id,
            message: Message::Request {
                request, channel, ..
            },
        } => requests::profile(
            context.swarm,
            context.service,
            context.store_queue,
            context.pow_permits,
            (id, connection_id),
            request,
            channel,
        ),
        Event::Message {
            peer: id,
            message:
                Message::Response {
                    request_id,
                    response,
                },
            ..
        } => context.replicas.profile_response(
            context.swarm,
            context.service,
            id,
            request_id,
            response,
        ),
        Event::OutboundFailure {
            peer,
            request_id,
            error,
            ..
        } => context.replicas.profile_failure(peer, request_id, &error),
        _ => {}
    }
}

fn keypackage(
    context: &mut Context<'_>,
    event: Event<proto::KeyPackageRequest, proto::KeyPackageResponse>,
) {
    match event {
        Event::Message {
            peer: id,
            connection_id,
            message: Message::Request {
                request, channel, ..
            },
        } => requests::keypackage(
            context.swarm,
            context.service,
            context.store_queue,
            id,
            connection_id,
            request,
            channel,
        ),
        Event::Message {
            peer: id,
            message:
                Message::Response {
                    request_id,
                    response,
                },
            ..
        } => context.replicas.keypackage_response(
            context.swarm,
            context.service,
            id,
            request_id,
            &response,
        ),
        Event::OutboundFailure {
            peer,
            request_id,
            error,
            ..
        } => context
            .replicas
            .keypackage_failure(peer, request_id, &error),
        _ => {}
    }
}

fn media(context: &mut Context<'_>, event: Event<proto::MediaRequest, proto::MediaResponse>) {
    match event {
        Event::Message {
            peer: id,
            connection_id,
            message: Message::Request {
                request, channel, ..
            },
        } => {
            *context.answered = (*context.answered).saturating_add(u64::from(requests::media(
                context.swarm,
                context.service,
                context.store_queue,
                context.replicas,
                (id, connection_id),
                request,
                channel,
            )));
        }
        Event::Message {
            peer: id,
            message:
                Message::Response {
                    request_id,
                    response,
                },
            ..
        } => context.replicas.media_response(
            context.swarm,
            context.service,
            id,
            request_id,
            response,
        ),
        Event::OutboundFailure {
            peer,
            request_id,
            error,
            ..
        } => {
            context.replicas.media_failure(
                context.swarm,
                context.service,
                peer,
                request_id,
                &error,
            );
        }
        _ => {}
    }
}

fn mailbox(context: &mut Context<'_>, event: Event<proto::MailboxRequest, proto::MailboxResponse>) {
    match event {
        Event::Message {
            peer: id,
            connection_id,
            message: Message::Request {
                request, channel, ..
            },
        } => requests::mailbox(
            context.swarm,
            context.service,
            context.store_queue,
            id,
            connection_id,
            request,
            channel,
        ),
        // The peer's answer to an inventory: it gets what it lacks.
        Event::Message {
            peer: id,
            message:
                Message::Response {
                    request_id,
                    response,
                },
            ..
        } => context.replicas.mailbox_response(
            context.swarm,
            context.service,
            id,
            request_id,
            response,
        ),
        Event::OutboundFailure {
            peer,
            request_id,
            error,
            ..
        } => context.replicas.mailbox_failure(peer, request_id, &error),
        _ => {}
    }
}

fn search_request(
    context: &mut Context<'_>,
    id: PeerId,
    connection_id: ConnectionId,
    request: proto::SearchRequest,
    channel: ResponseChannel<SearchResponse>,
) {
    let outcome = search_jobs::request(
        search_jobs::RequestContext {
            swarm: context.swarm,
            service: context.service,
            queue: context.search_queue,
            running: context.searching,
            done: context.search_done,
            pow_permits: context.pow_permits,
            pow_done: context.pow_done,
        },
        id,
        connection_id,
        request,
        channel,
    );
    *context.answered = (*context.answered).saturating_add(outcome.answered);
    *context.failed = (*context.failed).saturating_add(outcome.failed);
}

fn search(context: &mut Context<'_>, event: Event<proto::SearchRequest, proto::SearchResponse>) {
    match event {
        Event::Message {
            peer: id,
            connection_id,
            message: Message::Request {
                request, channel, ..
            },
        } => search_request(context, id, connection_id, request, channel),
        // A catch-up get: apply what came, queue again what a reply cut at 1 MiB left out.
        // A reply with no records ends the chunk, so keys the peer no longer has drop out.
        Event::Message {
            message:
                Message::Response {
                    request_id,
                    response,
                },
            ..
        } => super::catch_up::search_response(
            context.swarm,
            context.service,
            super::catch_up::SearchState {
                queue: context.catch_up_queue,
                asked: context.catch_up_asked,
                at: context.catch_up_at,
            },
            context.pow_permits,
            context.pow_done,
            request_id,
            &response,
        ),
        Event::OutboundFailure {
            peer: id,
            request_id,
            ..
        } => super::catch_up::search_failure(
            context.swarm,
            context.catch_up_queue,
            context.catch_up_asked,
            id,
            request_id,
        ),
        _ => {}
    }
}

fn inventory(
    context: &mut Context<'_>,
    event: Event<proto::InventoryRequest, proto::InventoryResponse>,
) {
    match event {
        Event::Message {
            peer: id,
            connection_id,
            message: Message::Request {
                request, channel, ..
            },
        } => requests::inventory(
            context.swarm,
            context.service,
            context.store_queue,
            id,
            connection_id,
            request,
            channel,
        ),
        // Catch-up: queue the keys this node lacks or holds older, and fetch the next page
        // while pages are full.
        Event::Message {
            peer: id,
            message:
                Message::Response {
                    request_id,
                    response,
                },
            ..
        } => super::catch_up::inventory_response(
            context.swarm,
            context.service,
            super::catch_up::InventoryState {
                asks: context.asks,
                queue: context.catch_up_queue,
                at: context.catch_up_at,
                refill: context.refill_round,
            },
            id,
            request_id,
            &response,
        ),
        Event::OutboundFailure { request_id, .. } => {
            super::catch_up::inventory_failure(context.swarm, context.asks, request_id);
        }
        _ => {}
    }
}

fn digest(context: &mut Context<'_>, event: Event<proto::DigestRequest, proto::DigestResponse>) {
    match event {
        Event::Message {
            peer: id,
            connection_id,
            message: Message::Request {
                request, channel, ..
            },
        } => requests::digest(
            context.swarm,
            context.service,
            context.store_queue,
            id,
            connection_id,
            request,
            channel,
        ),
        // Catch-up digests: ask what differs, or on failure the range the digest covered.
        event => super::catch_up::digest(
            context.swarm,
            context.service,
            context.asks,
            context.refill_round,
            event,
        ),
    }
}

fn gossip(context: &mut Context<'_>, event: gossipsub::Event) {
    if let gossipsub::Event::Message {
        propagation_source,
        message_id,
        message,
    } = event
    {
        pow::receive_gossip(
            context.swarm,
            context.service,
            context.pow_permits,
            context.pow_done,
            context.pow_gossip,
            (propagation_source, message_id, message.data),
        );
    }
}

fn kad_event(context: &mut Context<'_>, event: kad::Event) {
    match event {
        event @ kad::Event::OutboundQueryProgressed {
            result: kad::QueryResult::StartProviding(_),
            ..
        } => providing(context, event),
        kad::Event::InboundRequest {
            request:
                kad::InboundRequest::AddProvider {
                    record: Some(record),
                },
        } => context.admission.add_provider(context.swarm, record),
        kad::Event::OutboundQueryProgressed { id, result, .. } => {
            query(context, id, result);
        }
        _ => {}
    }
}

fn providing(context: &mut Context<'_>, event: kad::Event) {
    let kad::Event::OutboundQueryProgressed { id, .. } = &event else {
        return;
    };
    if *context.turn_query == Some(*id) {
        provided(
            event,
            context.turn_query,
            context.turn_unannounced,
            "TURN relay announced",
            "TURN relay not announced",
        );
    } else if *context.search_query == Some(*id) {
        provided(
            event,
            context.search_query,
            context.search_unannounced,
            "search node announced",
            "search node not announced",
        );
    }
}

fn provided(
    event: kad::Event,
    query: &mut Option<kad::QueryId>,
    unannounced: &mut bool,
    success: &str,
    failure: &str,
) {
    let kad::Event::OutboundQueryProgressed {
        result: kad::QueryResult::StartProviding(result),
        ..
    } = event
    else {
        return;
    };
    *query = None;
    match result {
        Ok(_) => announced(unannounced, success),
        Err(error) => tracing::warn!(%error, "{failure}"),
    }
}

fn announced(unannounced: &mut bool, message: &str) {
    *unannounced = false;
    tracing::info!("{message}");
}

fn query(context: &mut Context<'_>, id: kad::QueryId, result: kad::QueryResult) {
    match result {
        // Only a search node with too few gossip peers looks for providers: dial them.
        kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {
            providers,
            ..
        })) => dial_providers(context.swarm, providers),
        kad::QueryResult::GetClosestPeers(result) => {
            context
                .replicas
                .closest(context.swarm, context.service, context.watchers, id, result);
        }
        kad::QueryResult::Bootstrap(Ok(ok)) if ok.num_remaining == 0 => context
            .replicas
            .republish
            .bootstrapped(context.service.clock.instant()),
        _ => {}
    }
}

fn dial_providers(swarm: &mut Swarm<Behaviour>, providers: HashSet<PeerId>) {
    for provider in providers {
        if provider != *swarm.local_peer_id()
            && !swarm.is_connected(&provider)
            && dyapp_p2p_net::id_has_pow(&provider, dyapp_p2p_net::id_pow_bits())
        {
            let _ = swarm.dial(provider);
        }
    }
}

fn identified(context: &mut Context<'_>, event: identify::Event) {
    match event {
        // Before any routing: a peer of another network enters no DHT and no anchors.
        identify::Event::Received {
            connection_id,
            peer_id,
            info,
        } if info.protocol_version != context.service.protocols.version => {
            *context.foreign_peers = (*context.foreign_peers).saturating_add(1);
            identified::drop_foreign(
                context.swarm,
                context.admission.dialed(connection_id),
                peer_id,
                &info.protocol_version,
            );
        }
        // Kademlia learns a dialer's address only from identify: without this a node
        // never routes to peers that joined through it.
        identify::Event::Received {
            connection_id,
            peer_id,
            info,
        } if info.protocols.contains(&context.service.protocols.kad)
            || info
                .protocols
                .contains(&context.service.protocols.search_kad) =>
        {
            if !identified::add_node(
                context.swarm,
                context.service,
                context.anchors,
                context.admission,
                connection_id,
                peer_id,
                info,
            ) {
                return;
            }
            announce_providers(context);
        }
        _ => {}
    }
}

fn announce_providers(context: &mut Context<'_>) {
    if *context.turn_unannounced && context.turn_query.is_none() {
        let key = kad::RecordKey::new(&dyapp_p2p_net::TURN_KEY);
        *context.turn_query = context.swarm.behaviour_mut().kad.start_providing(key).ok();
    }
    if *context.search_unannounced && context.search_query.is_none() {
        let key = kad::RecordKey::new(&SEARCH_KEY);
        *context.search_query = context.swarm.behaviour_mut().kad.start_providing(key).ok();
    }
}

fn established(
    context: &mut Context<'_>,
    peer_id: PeerId,
    connection_id: ConnectionId,
    endpoint: &libp2p::core::ConnectedPoint,
) {
    context
        .replicas
        .trust
        .seen
        .entry(peer_id)
        .or_insert_with(|| context.service.clock.instant());
    context.admission.established(
        context.swarm,
        connection_id,
        peer_id,
        endpoint,
        context.service.clock.instant(),
    );
    drop_banned(context.swarm, context.service, peer_id);
}

fn closed(
    context: &mut Context<'_>,
    peer_id: PeerId,
    connection_id: ConnectionId,
    num_established: u32,
) {
    if num_established == 0 {
        context.catch_up_queue.remove(&peer_id);
        context.asks.forget(&peer_id);
        context.service.search_peer(&peer_id.to_string(), false);
        if !dyapp_p2p_net::routed(context.swarm, &peer_id) {
            context.replicas.trust.forget(&peer_id);
        }
    }
    context.nonces.remove(&connection_id);
    context
        .admission
        .closed(connection_id, peer_id, num_established == 0);
    // ponytail: a scan of all watchers per closed connection; index them by
    // connection if many devices watch at once.
    context.watchers.retain(|_, list| {
        list.retain(|(_, connection)| *connection != connection_id);
        !list.is_empty()
    });
}
