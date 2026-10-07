//! libp2p node: QUIC and TCP+Noise+Yamux, Kademlia, identify, AutoNAT.

use libp2p::identity::Keypair;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{autonat, identify, kad, noise, tcp, yamux, StreamProtocol, Swarm, SwarmBuilder};
use std::time::Duration;

pub const KAD_PROTOCOL: StreamProtocol = StreamProtocol::new("/dyapp/kad");
pub const IDENTIFY_PROTOCOL: &str = "/dyapp";

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
    pub identify: identify::Behaviour,
    pub autonat: autonat::Behaviour,
}

/// Kademlia role of the node.
pub enum Mode {
    /// DHT server once AutoNAT or the operator confirms an external address.
    Auto,
    /// Never answers DHT queries: mobile and other short-lived clients.
    Client,
}

pub fn build_swarm(keypair: Keypair, mode: Mode) -> anyhow::Result<Swarm<Behaviour>> {
    Ok(SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| {
            let peer_id = key.public().to_peer_id();
            let mut kad = kad::Behaviour::with_config(
                peer_id,
                kad::store::MemoryStore::new(peer_id),
                kad::Config::new(KAD_PROTOCOL),
            );
            kad.set_mode(match mode {
                Mode::Auto => None,
                Mode::Client => Some(kad::Mode::Client),
            });
            Behaviour {
                kad,
                identify: identify::Behaviour::new(identify::Config::new(
                    IDENTIFY_PROTOCOL.into(),
                    key.public(),
                )),
                autonat: autonat::Behaviour::new(peer_id, autonat::Config::default()),
            }
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::futures::StreamExt;
    use libp2p::swarm::SwarmEvent;
    use libp2p::Multiaddr;

    #[tokio::test]
    async fn client_learns_server_through_identify() {
        let mut server = build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
        server
            .listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap())
            .unwrap();
        let addr: Multiaddr = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address;
            }
        };
        // A confirmed external address switches Kademlia to server mode.
        server.add_external_address(addr.clone());

        let mut client = build_swarm(Keypair::generate_ed25519(), Mode::Client).unwrap();
        client.dial(addr).unwrap();
        let server_id = *server.local_peer_id();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                tokio::select! {
                    _ = server.select_next_some() => {}
                    event = client.select_next_some() => {
                        if let SwarmEvent::Behaviour(BehaviourEvent::Kad(
                            kad::Event::RoutingUpdated { peer, .. },
                        )) = event
                        {
                            if peer == server_id {
                                return;
                            }
                        }
                    }
                }
            }
        })
        .await
        .expect("client did not add the server to its routing table");
    }
}
