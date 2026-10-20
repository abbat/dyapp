//! Cheap admission for outbound attempts with a known peer ID.

use crate::DialGuard;
use libp2p::core::{transport::PortUse, Endpoint};
use libp2p::swarm::{
    dummy, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use std::convert::Infallible;
use std::io;
use std::task::{Context, Poll};

/// Refuses known outbound peers lacking node-ID proof, independently of the IP policy switch.
pub struct PowGate {
    guard: DialGuard,
    required_bits: fn() -> u32,
}

impl PowGate {
    /// Uses the process node-ID difficulty at the time of each outbound attempt.
    pub fn new(guard: DialGuard) -> Self {
        Self {
            guard,
            required_bits: crate::id_pow_bits,
        }
    }

    /// The shared guard: its `take_denied` counts transport and peer-proof refusals together.
    pub fn guard(&self) -> &DialGuard {
        &self.guard
    }
}

impl NetworkBehaviour for PowGate {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_pending_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer: Option<PeerId>,
        _: &[Multiaddr],
        _: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        if peer.is_some_and(|peer| !crate::id_has_pow(&peer, (self.required_bits)())) {
            self.guard.refuse();
            return Err(ConnectionDenied::new(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "outbound peer lacks node-ID proof of work",
            )));
        }
        Ok(Vec::new())
    }

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, _: FromSwarm) {}

    fn on_connection_handler_event(
        &mut self,
        _: PeerId,
        _: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {}
    }

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer_with_proof(valid: bool) -> PeerId {
        loop {
            let peer = libp2p::identity::Keypair::generate_ed25519()
                .public()
                .to_peer_id();
            if crate::id_has_pow(&peer, 4) == valid {
                return peer;
            }
        }
    }

    #[test]
    fn only_known_weak_outbound_peers_are_refused_with_shared_counts() {
        for enabled in [true, false] {
            let guard = DialGuard::new(enabled);
            let mut gate = PowGate {
                guard: guard.clone(),
                required_bits: || 4,
            };
            let id = ConnectionId::new_unchecked(1);
            let address: Multiaddr = "/ip4/192.0.2.1/tcp/7".parse().unwrap();
            let addresses = [address.clone()];
            let weak = peer_with_proof(false);
            assert!(gate
                .handle_pending_outbound_connection(id, Some(weak), &addresses, Endpoint::Dialer)
                .is_err());
            assert_eq!(guard.take_denied(), 1);
            assert_eq!(guard.take_denied(), 0);
            for peer in [None, Some(peer_with_proof(true))] {
                assert!(gate
                    .handle_pending_outbound_connection(id, peer, &addresses, Endpoint::Dialer)
                    .unwrap()
                    .is_empty());
            }
            gate.handle_pending_inbound_connection(id, &address, &address)
                .unwrap();
            gate.handle_established_inbound_connection(id, weak, &address, &address)
                .unwrap();
            gate.handle_established_outbound_connection(
                id,
                weak,
                &address,
                Endpoint::Dialer,
                PortUse::Reuse,
            )
            .unwrap();
            assert_eq!(guard.take_denied(), 0);
            let waker = libp2p::futures::task::noop_waker();
            assert!(gate.poll(&mut Context::from_waker(&waker)).is_pending());
        }
    }

    #[test]
    fn difficulty_is_read_on_each_attempt() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static BITS: AtomicU32 = AtomicU32::new(0);
        let guard = DialGuard::new(true);
        let mut gate = PowGate {
            guard: guard.clone(),
            required_bits: || BITS.load(Ordering::Relaxed),
        };
        let weak = peer_with_proof(false);
        let id = ConnectionId::new_unchecked(1);
        assert!(gate
            .handle_pending_outbound_connection(id, Some(weak), &[], Endpoint::Dialer)
            .is_ok());
        BITS.store(4, Ordering::Relaxed);
        assert!(gate
            .handle_pending_outbound_connection(id, Some(weak), &[], Endpoint::Dialer)
            .is_err());
        assert_eq!(guard.take_denied(), 1);
    }

    // On a real swarm: a request to a disconnected weak peer fails before the transport, while
    // the same peer, once connected inbound, gets the request on that connection.
    #[tokio::test]
    async fn weak_peer_is_not_dialed_but_served_on_its_inbound_connection() {
        use crate::{
            build_configured_swarm, build_swarm, proto, BehaviourEvent, Mode, SwarmOptions,
        };
        use libp2p::futures::StreamExt;
        use libp2p::request_response::{Event, Message, OutboundFailure};
        use libp2p::swarm::{behaviour::toggle::Toggle, SwarmEvent};
        use std::time::Duration;

        let keypair = libp2p::identity::Keypair::generate_ed25519;
        let guard = DialGuard::new(false);
        let options = SwarmOptions {
            dial_guard: Some(guard.clone()),
            ..SwarmOptions::default()
        };
        let mut node = build_configured_swarm(keypair(), options).unwrap();
        // No peer ID carries 64 bits of proof, so every peer is weak.
        node.behaviour_mut().pow_gate = Toggle::from(Some(PowGate {
            guard: guard.clone(),
            required_bits: || 64,
        }));
        let mut weak = build_swarm(keypair(), Mode::Auto).unwrap();
        let weak_id = *weak.local_peer_id();
        let request = || proto::NodeRequest {
            request: Some(proto::node_request::Request::Info(proto::InfoRequest {})),
        };
        let listen = |swarm: &mut libp2p::Swarm<crate::Behaviour>| {
            swarm
                .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
                .unwrap();
        };
        listen(&mut weak);
        listen(&mut node);
        let wait = Duration::from_secs(10);

        let weak_address = tokio::time::timeout(wait, async {
            loop {
                if let SwarmEvent::NewListenAddr { address, .. } = weak.select_next_some().await {
                    return address;
                }
            }
        })
        .await
        .unwrap();
        node.add_peer_address(weak_id, weak_address);
        node.behaviour_mut().node.send_request(&weak_id, request());
        let (node_address, failure) = tokio::time::timeout(wait, async {
            let (mut address, mut failure) = (None::<Multiaddr>, None::<OutboundFailure>);
            loop {
                if let (Some(address), Some(failure)) = (&address, &failure) {
                    return (address.clone(), format!("{failure:?}"));
                }
                tokio::select! {
                    event = node.select_next_some() => match event {
                        SwarmEvent::NewListenAddr { address: a, .. } => address = Some(a),
                        SwarmEvent::Behaviour(BehaviourEvent::Node(Event::OutboundFailure {
                            error, ..
                        })) => failure = Some(error),
                        _ => {}
                    },
                    event = weak.select_next_some() => {
                        assert!(!matches!(event, SwarmEvent::ConnectionEstablished { .. }));
                    }
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(failure, format!("{:?}", OutboundFailure::DialFailure));
        assert!(guard.take_denied() > 0);

        weak.dial(node_address).unwrap();
        let mut sent = false;
        tokio::time::timeout(wait, async {
            loop {
                tokio::select! {
                    event = node.select_next_some() => match event {
                        SwarmEvent::ConnectionEstablished { peer_id, .. } if !sent => {
                            assert_eq!(peer_id, weak_id);
                            node.behaviour_mut().node.send_request(&weak_id, request());
                            sent = true;
                        }
                        SwarmEvent::Behaviour(BehaviourEvent::Node(Event::Message {
                            message: Message::Response { .. },
                            ..
                        })) => return,
                        SwarmEvent::Behaviour(BehaviourEvent::Node(Event::OutboundFailure {
                            error, ..
                        })) => panic!("{error:?}"),
                        _ => {}
                    },
                    event = weak.select_next_some() => {
                        if let SwarmEvent::Behaviour(BehaviourEvent::Node(Event::Message {
                            message: Message::Request { channel, .. },
                            ..
                        })) = event
                        {
                            let response = proto::NodeResponse {
                                status: proto::Status::Ok.into(),
                                ..Default::default()
                            };
                            weak.behaviour_mut().node.send_response(channel, response).unwrap();
                        }
                    }
                }
            }
        })
        .await
        .unwrap();
    }
}
