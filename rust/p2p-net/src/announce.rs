//! Identify that announces only global listen addresses, or every one when none is global, so a
//! node on `0.0.0.0` or `[::]` does not hand out its LAN, Docker and VPN addresses.

use libp2p::core::transport::{ListenerId, PortUse};
use libp2p::core::Endpoint;
use libp2p::identify;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::behaviour::{ExpiredListenAddr, NewListenAddr};
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use std::task::{Context, Poll};

/// Whether `address` is a global one: a name, or an IP outside the dial guard's deny ranges.
fn is_global(address: &Multiaddr) -> bool {
    match address.iter().next() {
        Some(Protocol::Ip4(ip)) => !crate::dial_guard::forbidden(ip.into()),
        Some(Protocol::Ip6(ip)) => !crate::dial_guard::forbidden(ip.into()),
        Some(Protocol::Dns(_) | Protocol::Dns4(_) | Protocol::Dns6(_)) => true,
        _ => false,
    }
}

/// [`identify::Behaviour`] that sees only the global listen addresses, or every one when none is
/// global.
pub struct Announce {
    inner: identify::Behaviour,
    listening: Vec<(ListenerId, Multiaddr)>,
    announced: Vec<(ListenerId, Multiaddr)>,
}

impl Announce {
    /// Wraps `inner`.
    pub fn new(inner: identify::Behaviour) -> Self {
        Self {
            inner,
            listening: Vec::new(),
            announced: Vec::new(),
        }
    }

    fn sync(&mut self) {
        let global = self.listening.iter().any(|(_, a)| is_global(a));
        let wanted: Vec<_> = self
            .listening
            .iter()
            .filter(|(_, a)| !global || is_global(a))
            .cloned()
            .collect();
        for (listener_id, addr) in &self.announced {
            if !wanted.iter().any(|(_, a)| a == addr) {
                self.inner
                    .on_swarm_event(FromSwarm::ExpiredListenAddr(ExpiredListenAddr {
                        listener_id: *listener_id,
                        addr,
                    }));
            }
        }
        for (listener_id, addr) in &wanted {
            if !self.announced.iter().any(|(_, a)| a == addr) {
                self.inner
                    .on_swarm_event(FromSwarm::NewListenAddr(NewListenAddr {
                        listener_id: *listener_id,
                        addr,
                    }));
            }
        }
        self.announced = wanted;
    }
}

impl NetworkBehaviour for Announce {
    type ConnectionHandler = <identify::Behaviour as NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = identify::Event;

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
        self.inner
            .handle_established_inbound_connection(id, peer, local, remote)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        id: ConnectionId,
        peer: Option<PeerId>,
        addresses: &[Multiaddr],
        role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        self.inner
            .handle_pending_outbound_connection(id, peer, addresses, role)
    }

    fn handle_established_outbound_connection(
        &mut self,
        id: ConnectionId,
        peer: PeerId,
        address: &Multiaddr,
        role: Endpoint,
        port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner
            .handle_established_outbound_connection(id, peer, address, role, port_use)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::NewListenAddr(e) => {
                self.listening.push((e.listener_id, e.addr.clone()));
                self.sync();
            }
            FromSwarm::ExpiredListenAddr(e) => {
                self.listening.retain(|(_, a)| a != e.addr);
                self.sync();
            }
            event => self.inner.on_swarm_event(event),
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer: PeerId,
        id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        self.inner.on_connection_handler_event(peer, id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        self.inner.poll(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addresses(list: &[&str]) -> Vec<Multiaddr> {
        list.iter().map(|a| a.parse().unwrap()).collect()
    }

    #[test]
    fn names_and_public_ips_are_global() {
        let global = |a: &str| is_global(&a.parse().unwrap());
        assert!(global("/ip4/8.8.8.8/tcp/1"));
        assert!(global("/dns4/node.example/tcp/1"));
        assert!(global("/ip6/2001:db8::1/tcp/1"));
        for local in [
            "/ip4/10.0.0.1/tcp/1",
            "/ip4/127.0.0.1/tcp/1",
            "/ip6/fe80::1/tcp/1",
            "/ip6/fd00::1/tcp/1",
            "/memory/1",
        ] {
            assert!(!global(local), "{local}");
        }
    }

    #[test]
    fn a_global_listen_address_replaces_private_ones_until_it_expires() {
        let key = libp2p::identity::Keypair::generate_ed25519();
        let mut announce = Announce::new(identify::Behaviour::new(identify::Config::new(
            "test".into(),
            key.public(),
        )));
        let (private, global) = (
            addresses(&["/ip4/10.0.0.1/tcp/1"]),
            addresses(&["/ip4/8.8.8.8/tcp/1"]),
        );
        let listener = ListenerId::next();
        let announced = |a: &Announce| {
            a.announced
                .iter()
                .map(|(_, a)| a.clone())
                .collect::<Vec<_>>()
        };
        for addr in [&private[0], &global[0]] {
            announce.on_swarm_event(FromSwarm::NewListenAddr(NewListenAddr {
                listener_id: listener,
                addr,
            }));
            if addr == &private[0] {
                assert_eq!(announced(&announce), private);
            }
        }
        assert_eq!(announced(&announce), global);
        announce.on_swarm_event(FromSwarm::ExpiredListenAddr(ExpiredListenAddr {
            listener_id: listener,
            addr: &global[0],
        }));
        assert_eq!(announced(&announce), private);
    }
}
