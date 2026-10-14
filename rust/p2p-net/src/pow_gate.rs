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
}
