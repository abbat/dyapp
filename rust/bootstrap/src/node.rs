//! The libp2p request loop of a node: answers the node protocol and deletes expired envelopes.

use crate::service::Service;
use dyapp_p2p_net::{Behaviour, BehaviourEvent};
use libp2p::futures::StreamExt;
use libp2p::request_response::{Event, Message};
use libp2p::swarm::{ConnectionId, SwarmEvent};
use libp2p::Swarm;
use std::collections::HashMap;
use std::time::Duration;

/// Serves requests on `swarm` until the task is dropped.
pub async fn run(mut swarm: Swarm<Behaviour>, service: Service) {
    // The mailbox challenge issued on each open connection.
    let mut nonces: HashMap<ConnectionId, [u8; 32]> = HashMap::new();
    let mut cleanup = tokio::time::interval(Duration::from_secs(3600));
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
                    let _ = swarm.behaviour_mut().node.send_response(channel, response);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::Message {
                    peer,
                    message: Message::Request { request, channel, .. },
                    ..
                })) => match service.profile(&peer.to_string(), request) {
                    Ok(response) => {
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
                            let _ = swarm.behaviour_mut().mailbox.send_response(channel, response);
                        }
                        Err(error) => tracing::error!(%error, "mailbox request failed"),
                    }
                }
                SwarmEvent::ConnectionClosed { connection_id, .. } => {
                    nonces.remove(&connection_id);
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
        }
    }
}
