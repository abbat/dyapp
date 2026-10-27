//! AutoNAT that reserves a circuit relay v2 slot while it reports the node private: it remembers
//! dialled peers whose identify lists the hop protocol and listens on a circuit through one of
//! them, then through another when that listener closes. A dial to a peer also tries a circuit
//! through each of them; smart dial starts circuits after the direct addresses.

use either::Either;
use libp2p::autonat;
use libp2p::core::transport::{ListenerId, PortUse};
use libp2p::core::upgrade::DeniedUpgrade;
use libp2p::core::Endpoint;
use libp2p::multiaddr::Protocol;
use libp2p::relay::HOP_PROTOCOL_NAME;
use libp2p::swarm::handler::{
    ConnectionEvent, ConnectionHandlerSelect, ProtocolsChange, SubstreamProtocol,
};
use libp2p::swarm::{
    ConnectionDenied, ConnectionHandler, ConnectionHandlerEvent, ConnectionId, FromSwarm,
    ListenOpts, NetworkBehaviour, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use std::collections::HashMap;
use std::convert::Infallible;
use std::task::{Context, Poll};

/// Reports once that the remote lists the hop protocol.
#[derive(Default)]
pub struct HopWatch {
    seen: bool,
    reported: bool,
}

impl ConnectionHandler for HopWatch {
    type FromBehaviour = Infallible;
    type ToBehaviour = ();
    type InboundProtocol = DeniedUpgrade;
    type OutboundProtocol = DeniedUpgrade;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<DeniedUpgrade> {
        SubstreamProtocol::new(DeniedUpgrade, ())
    }

    fn on_behaviour_event(&mut self, event: Infallible) {
        match event {}
    }

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<ConnectionHandlerEvent<DeniedUpgrade, (), ()>> {
        if self.seen && !self.reported {
            self.reported = true;
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(()));
        }
        Poll::Pending
    }

    fn on_connection_event(&mut self, event: ConnectionEvent<DeniedUpgrade, DeniedUpgrade>) {
        if let ConnectionEvent::RemoteProtocolsChange(ProtocolsChange::Added(mut added)) = event {
            self.seen |= added.any(|p| *p == HOP_PROTOCOL_NAME);
        }
    }
}

/// [`autonat::Behaviour`] that listens on a relay circuit while the NAT status is private.
pub struct AutoRelay {
    inner: autonat::Behaviour,
    /// Remote addresses of our own direct dials.
    dialled: HashMap<ConnectionId, Multiaddr>,
    /// Connected relays, an address we reached each on, and whether it may take a reservation.
    relays: HashMap<PeerId, (Multiaddr, bool)>,
    listener: Option<ListenerId>,
}

impl AutoRelay {
    /// Wraps `inner`.
    pub fn new(inner: autonat::Behaviour) -> Self {
        Self {
            inner,
            dialled: HashMap::new(),
            relays: HashMap::new(),
            listener: None,
        }
    }

    /// The circuit to `peer` through each connected relay but itself: a dialer cannot know which
    /// relay holds the peer's reservation, and one without it refuses at once.
    fn circuits(&self, peer: PeerId) -> Vec<Multiaddr> {
        self.relays
            .iter()
            .filter(|(relay, _)| **relay != peer)
            .map(|(relay, (address, _))| circuit(address.clone(), *relay))
            .collect()
    }

    fn closed(&mut self, id: ListenerId) {
        if self.listener == Some(id) {
            self.listener = None;
        }
    }
}

/// `address` of `relay`, without a trailing peer ID, extended to a circuit through it.
fn circuit(mut address: Multiaddr, relay: PeerId) -> Multiaddr {
    if let Some(Protocol::P2p(_)) = address.iter().last() {
        address.pop();
    }
    address
        .with(Protocol::P2p(relay))
        .with(Protocol::P2pCircuit)
}

impl NetworkBehaviour for AutoRelay {
    type ConnectionHandler = ConnectionHandlerSelect<THandler<autonat::Behaviour>, HopWatch>;
    type ToSwarm = autonat::Event;

    fn handle_pending_inbound_connection(
        &mut self,
        id: ConnectionId,
        local: &Multiaddr,
        remote: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.inner
            .handle_pending_inbound_connection(id, local, remote)
    }

    fn handle_established_inbound_connection(
        &mut self,
        id: ConnectionId,
        peer: PeerId,
        local: &Multiaddr,
        remote: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(self
            .inner
            .handle_established_inbound_connection(id, peer, local, remote)?
            .select(HopWatch::default()))
    }

    fn handle_pending_outbound_connection(
        &mut self,
        id: ConnectionId,
        peer: Option<PeerId>,
        addresses: &[Multiaddr],
        role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let mut found = self
            .inner
            .handle_pending_outbound_connection(id, peer, addresses, role)?;
        // A hole punch dials as listener at given addresses.
        if let (Some(peer), Endpoint::Dialer) = (peer, role) {
            found.extend(self.circuits(peer));
        }
        Ok(found)
    }

    fn handle_established_outbound_connection(
        &mut self,
        id: ConnectionId,
        peer: PeerId,
        address: &Multiaddr,
        role: Endpoint,
        port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let handler = self
            .inner
            .handle_established_outbound_connection(id, peer, address, role, port_use)?;
        // A circuit cannot carry another reservation.
        if !crate::is_circuit(address) {
            self.dialled.insert(id, address.clone());
        }
        Ok(handler.select(HopWatch::default()))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionClosed(e) => {
                self.dialled.remove(&e.connection_id);
                if e.remaining_established == 0 {
                    self.relays.remove(&e.peer_id);
                }
            }
            FromSwarm::ListenerClosed(e) => self.closed(e.listener_id),
            FromSwarm::ListenerError(e) => self.closed(e.listener_id),
            _ => {}
        }
        self.inner.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer: PeerId,
        id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            Either::Left(event) => self.inner.on_connection_handler_event(peer, id, event),
            Either::Right(()) => {
                if let Some(address) = self.dialled.get(&id) {
                    self.relays.insert(peer, (address.clone(), true));
                }
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Poll::Ready(action) = self.inner.poll(cx) {
            return Poll::Ready(action.map_in(Either::Left));
        }
        let private = matches!(self.inner.nat_status(), autonat::NatStatus::Private);
        match self.listener {
            Some(id) if !private => {
                self.listener = None;
                Poll::Ready(ToSwarm::RemoveListener { id })
            }
            None if private => {
                // ponytail: one reservation through any relay, fine for a handful of bootstraps.
                let Some((peer, (address, candidate))) = self
                    .relays
                    .iter_mut()
                    .find(|(_, (_, candidate))| *candidate)
                else {
                    return Poll::Pending;
                };
                // A refusing relay is not retried in a loop; a new connection makes it a
                // candidate again.
                *candidate = false;
                let opts = ListenOpts::new(circuit(address.clone(), *peer));
                self.listener = Some(opts.listener_id());
                Poll::Ready(ToSwarm::ListenOn { opts })
            }
            _ => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{build_configured_swarm, BehaviourEvent, Mode, SwarmOptions};
    use libp2p::core::Endpoint;
    use libp2p::futures::StreamExt;
    use libp2p::identity::Keypair;
    use libp2p::multiaddr::Protocol;
    use libp2p::swarm::{ConnectionId, NetworkBehaviour, SwarmEvent};
    use libp2p::{autonat, relay, Multiaddr, PeerId};
    use std::time::Duration;

    #[test]
    fn a_relay_circuit_never_enters_either_dht() {
        let mut swarm = crate::build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
        let (peer, relay) = (PeerId::random(), PeerId::random());
        let direct: Multiaddr = "/ip4/198.51.100.1/tcp/7070".parse().unwrap();
        let circuit = super::circuit(direct.clone(), relay);
        assert!(!crate::add_peer(&mut swarm, peer, circuit.clone()));
        assert!(!crate::add_search_peer(&mut swarm, peer, circuit));
        assert!(crate::add_peer(&mut swarm, peer, direct));
    }

    #[tokio::test]
    async fn a_private_node_listens_on_a_circuit_through_a_connected_relay() {
        let make = |mode, relay| {
            let autonat = autonat::Config {
                boot_delay: Duration::from_millis(100),
                retry_interval: Duration::from_millis(500),
                throttle_server_period: Duration::ZERO,
                confidence_max: 0,
                only_global_ips: false,
                ..Default::default()
            };
            build_configured_swarm(
                Keypair::generate_ed25519(),
                SwarmOptions {
                    mode,
                    relay,
                    autonat,
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let mut relay = make(Mode::Auto, Some(relay::Config::default()));
        let mut node = make(Mode::Client, None);
        relay
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let address = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = relay.select_next_some().await {
                break address.with(Protocol::P2p(*relay.local_peer_id()));
            }
        };
        relay.add_external_address(address.clone());
        // Without a listener the dial-back to the observed address fails: AutoNAT says private.
        node.dial(address.clone()).unwrap();
        let (mut private, mut circuit) = (false, None);
        tokio::time::timeout(Duration::from_secs(30), async {
            while circuit.is_none() {
                tokio::select! {
                    event = relay.select_next_some() => { let _ = event; }
                    event = node.select_next_some() => match event {
                        SwarmEvent::Behaviour(BehaviourEvent::Autonat(
                            autonat::Event::StatusChanged { new, .. },
                        )) => private |= new == autonat::NatStatus::Private,
                        SwarmEvent::NewListenAddr { address, .. } => circuit = Some(address),
                        _ => {}
                    },
                }
            }
        })
        .await
        .unwrap();
        let circuit = circuit.unwrap();
        assert!(private);
        assert!(crate::is_circuit(&circuit), "{circuit}");

        // A dial to another peer adds the circuit through the relay after its direct address.
        fn dial(node: &mut libp2p::Swarm<crate::Behaviour>, peer: PeerId) -> Vec<Multiaddr> {
            node.behaviour_mut()
                .handle_pending_outbound_connection(
                    ConnectionId::new_unchecked(usize::MAX),
                    Some(peer),
                    &[],
                    Endpoint::Dialer,
                )
                .unwrap()
        }
        let relay_peer = *relay.local_peer_id();
        assert!(dial(&mut node, relay_peer)
            .iter()
            .all(|a| !crate::is_circuit(a)));
        let target = PeerId::random();
        let direct: Multiaddr = "/ip4/198.51.100.1/tcp/7070".parse().unwrap();
        node.behaviour_mut()
            .kad
            .add_address(&target, direct.clone());
        let found = dial(&mut node, target);
        let circuit = super::circuit(address, relay_peer);
        assert_eq!(found, [direct.with(Protocol::P2p(target)), circuit]);
    }
}
