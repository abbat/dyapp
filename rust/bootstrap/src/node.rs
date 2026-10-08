//! The libp2p request loop of a node: answers the node protocol and deletes expired envelopes.

use crate::config::Limits;
use crate::service::Service;
use dyapp_p2p_net::{build_limited_swarm, Behaviour, BehaviourEvent, Mode};
use libp2p::connection_limits::ConnectionLimits;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::request_response::{Event, Message};
use libp2p::swarm::{ConnectionId, ListenError, SwarmEvent};
use libp2p::Swarm;
use prost::Message as _;
use std::collections::HashMap;
use std::time::Duration;

/// The node's swarm with the connection and stream limits from the config.
/// ponytail: no memory-use threshold (libp2p memory-connection-limits is not vendored); the
/// connection, stream and message caps bound memory instead.
pub fn swarm(keypair: Keypair, limits: &Limits) -> anyhow::Result<Swarm<Behaviour>> {
    let connections = ConnectionLimits::default()
        .with_max_established(Some(limits.max_connections))
        .with_max_pending_incoming(Some(limits.max_connections))
        .with_max_established_per_peer(Some(limits.max_connections_per_peer));
    build_limited_swarm(keypair, Mode::Auto, connections, limits.max_streams)
}

/// Serves requests on `swarm` until the task is dropped.
pub async fn run(mut swarm: Swarm<Behaviour>, service: Service) {
    // The mailbox challenge issued on each open connection.
    let mut nonces: HashMap<ConnectionId, [u8; 32]> = HashMap::new();
    // Incoming connections refused by the connection limits since the last maintenance run.
    let mut refused = 0u64;
    let mut cleanup = tokio::time::interval(Duration::from_secs(3600));
    let maintenance = service.config.maintenance.clone();
    let mut maintain = tokio::time::interval(Duration::from_secs(
        u64::from(maintenance.interval_minutes) * 60,
    ));
    loop {
        tokio::select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => tracing::info!(%address, "listening"),
                SwarmEvent::ExternalAddrConfirmed { address } => {
                    tracing::info!(%address, "external address confirmed")
                }
                // ponytail: SQLite calls block the swarm loop; move them to spawn_blocking when
                // load makes request latency visible.
                SwarmEvent::Behaviour(BehaviourEvent::Node(Event::Message {
                    message: Message::Request { request, channel, .. },
                    ..
                })) => {
                    let response = service.node(request);
                    service.traffic.add(response.encoded_len() as u64);
                    let _ = swarm.behaviour_mut().node.send_response(channel, response);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::Message {
                    peer,
                    message: Message::Request { request, channel, .. },
                    ..
                })) => match service.profile(&peer.to_string(), request) {
                    Ok(response) => {
                        service.traffic.add(response.encoded_len() as u64);
                        let _ = swarm.behaviour_mut().profile.send_response(channel, response);
                    }
                    // Dropping the channel fails the request; the client tries another node.
                    Err(error) => tracing::error!(%error, "profile request failed"),
                },
                SwarmEvent::Behaviour(BehaviourEvent::Mailbox(Event::Message {
                    peer,
                    connection_id,
                    message: Message::Request { request, channel, .. },
                })) => {
                    let mut nonce = nonces.remove(&connection_id);
                    let result = service.mailbox(&peer.to_string(), &mut nonce, request);
                    if let Some(nonce) = nonce {
                        nonces.insert(connection_id, nonce);
                    }
                    match result {
                        Ok(response) => {
                            service.traffic.add(response.encoded_len() as u64);
                            let _ = swarm.behaviour_mut().mailbox.send_response(channel, response);
                        }
                        Err(error) => tracing::error!(%error, "mailbox request failed"),
                    }
                }
                SwarmEvent::ConnectionClosed { connection_id, .. } => {
                    nonces.remove(&connection_id);
                }
                SwarmEvent::IncomingConnectionError { error: ListenError::Denied { .. }, .. } => {
                    refused += 1;
                }
                _ => {}
            },
            _ = cleanup.tick() => {
                let now = chrono::Utc::now().timestamp();
                match service.store.cleanup_expired(now) {
                    Ok(removed) => tracing::info!(removed, "expired messages removed"),
                    Err(error) => tracing::error!(%error, "message cleanup failed"),
                }
            }
            _ = maintain.tick() => {
                match service.store.maintain(maintenance.vacuum_pages) {
                    Ok([profiles, messages]) => {
                        tracing::info!(profiles, messages, "store maintenance done, free pages left")
                    }
                    Err(error) => tracing::error!(%error, "store maintenance failed"),
                }
                if let Err(error) = service.traffic.save() {
                    tracing::error!(%error, "traffic count not saved");
                }
                if refused > 0 {
                    tracing::warn!(refused, "connections refused by the connection limits");
                    refused = 0;
                }
            }
        }
    }
}
