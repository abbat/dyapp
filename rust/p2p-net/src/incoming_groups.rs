//! One budget for pending and established incoming connections from an IP group.

use ipnet::IpNet;
use libp2p::core::{transport::PortUse, Endpoint};
use libp2p::multiaddr::Protocol;
use libp2p::swarm::{
    dummy, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use std::collections::HashMap;
use std::convert::Infallible;
use std::task::{Context, Poll};

/// Incoming connection budget and the remote IP masks that share it.
pub struct IncomingGroupLimits {
    max: u32,
    ipv4_prefix: u8,
    ipv6_prefix: u8,
}

impl IncomingGroupLimits {
    pub fn new(max: u32, ipv4_prefix: u8, ipv6_prefix: u8) -> anyhow::Result<Self> {
        anyhow::ensure!(max > 0, "incoming group connection limit must be positive");
        anyhow::ensure!(
            ipv4_prefix <= 32 && ipv6_prefix <= 128,
            "invalid incoming group IP prefix"
        );
        Ok(Self {
            max,
            ipv4_prefix,
            ipv6_prefix,
        })
    }

    fn group(&self, address: &Multiaddr) -> Option<IpNet> {
        address.iter().find_map(|part| match part {
            Protocol::Ip4(ip) => IpNet::new(ip.into(), self.ipv4_prefix)
                .ok()
                .map(|net| net.trunc()),
            Protocol::Ip6(ip) => IpNet::new(ip.into(), self.ipv6_prefix)
                .ok()
                .map(|net| net.trunc()),
            _ => None,
        })
    }
}

/// Reserves a slot before the handshake and keeps it until failure or close.
/// Endpoints without an IP share one bounded group. Outgoing connections are exempt.
pub struct IncomingGroupGuard {
    limits: IncomingGroupLimits,
    connections: HashMap<ConnectionId, Option<IpNet>>,
    groups: HashMap<Option<IpNet>, u32>,
    denied: u64,
}

impl IncomingGroupGuard {
    pub fn new(limits: IncomingGroupLimits) -> Self {
        Self {
            limits,
            connections: HashMap::new(),
            groups: HashMap::new(),
            denied: 0,
        }
    }

    /// Pending and established incoming slots currently reserved.
    pub fn active_connections(&self) -> usize {
        self.connections.len()
    }

    /// IP groups with at least one reserved slot.
    pub fn active_groups(&self) -> usize {
        self.groups.len()
    }

    /// Refused attempts since the previous call.
    pub fn take_denied(&mut self) -> u64 {
        std::mem::take(&mut self.denied)
    }

    fn reserve(&mut self, id: ConnectionId, address: &Multiaddr) -> Result<(), ConnectionDenied> {
        if self.connections.contains_key(&id) {
            return Ok(());
        }
        let group = self.limits.group(address);
        let count = self.groups.get(&group).copied().unwrap_or(0);
        if count >= self.limits.max {
            self.denied = self.denied.saturating_add(1);
            return Err(ConnectionDenied::new(std::io::Error::other(
                "incoming IP group connection limit",
            )));
        }
        self.groups.insert(group, count + 1);
        self.connections.insert(id, group);
        Ok(())
    }

    fn release(&mut self, id: ConnectionId) {
        if let Some(group) = self.connections.remove(&id) {
            if let Some(count) = self.groups.get_mut(&group) {
                *count -= 1;
                if *count == 0 {
                    self.groups.remove(&group);
                }
            }
        }
    }
}

impl NetworkBehaviour for IncomingGroupGuard {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_pending_inbound_connection(
        &mut self,
        id: ConnectionId,
        _: &Multiaddr,
        remote: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.reserve(id, remote)
    }

    fn handle_established_inbound_connection(
        &mut self,
        id: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        remote: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        // Retain the pending reservation; a composed refusal emits ListenFailure (swarm 0.48).
        self.reserve(id, remote)?;
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

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ListenFailure(event) => self.release(event.connection_id),
            FromSwarm::ConnectionClosed(event) => self.release(event.connection_id),
            _ => {}
        }
    }

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
    use libp2p::core::ConnectedPoint;
    use libp2p::swarm::behaviour::{ConnectionClosed, ListenFailure};
    use libp2p::swarm::ListenError;

    #[test]
    fn pending_and_established_share_slots_and_failure_or_close_releases_once() {
        let mut guard = IncomingGroupGuard::new(IncomingGroupLimits::new(32, 24, 48).unwrap());
        let local: Multiaddr = "/ip4/127.0.0.1/tcp/7".parse().unwrap();
        let remote: Multiaddr = "/ip4/192.0.2.1/tcp/9".parse().unwrap();
        let peer = PeerId::random();
        let ids: Vec<_> = (0..32).map(ConnectionId::new_unchecked).collect();
        for (i, id) in ids.iter().enumerate() {
            guard
                .handle_pending_inbound_connection(*id, &local, &remote)
                .unwrap();
            if i % 2 == 0 {
                guard
                    .handle_established_inbound_connection(*id, peer, &local, &remote)
                    .unwrap();
            }
        }
        assert_eq!(guard.connections.len(), 32);
        let refused = ConnectionId::new_unchecked(32);
        assert!(guard
            .handle_pending_inbound_connection(refused, &local, &remote)
            .is_err());
        assert_eq!(guard.take_denied(), 1);
        assert_eq!(guard.take_denied(), 0);
        // Same connection never charges twice, even while the budget is full.
        guard
            .handle_pending_inbound_connection(ids[0], &local, &remote)
            .unwrap();
        guard
            .handle_established_inbound_connection(ids[0], peer, &local, &remote)
            .unwrap();
        guard
            .handle_established_outbound_connection(
                ConnectionId::new_unchecked(33),
                peer,
                &remote,
                Endpoint::Dialer,
                PortUse::New,
            )
            .unwrap();
        assert_eq!(guard.connections.len(), 32);

        let error = ListenError::Denied {
            cause: ConnectionDenied::new(std::io::Error::other("other guard")),
        };
        for id in ids.iter().skip(1).step_by(2) {
            let event = FromSwarm::ListenFailure(ListenFailure {
                local_addr: &local,
                send_back_addr: &remote,
                error: &error,
                connection_id: *id,
                peer_id: Some(peer),
            });
            guard.on_swarm_event(event);
            guard.on_swarm_event(event);
        }
        let endpoint = ConnectedPoint::Listener {
            local_addr: local.clone(),
            send_back_addr: remote.clone(),
        };
        for id in ids.iter().step_by(2) {
            let event = FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id: peer,
                connection_id: *id,
                endpoint: &endpoint,
                cause: None,
                remaining_established: 0,
            });
            guard.on_swarm_event(event);
            guard.on_swarm_event(event);
        }
        assert!(guard.connections.is_empty() && guard.groups.is_empty());
        // A preauthenticated transport may reach established admission without a pending callback.
        guard
            .handle_established_inbound_connection(refused, peer, &local, &remote)
            .unwrap();
        assert_eq!(guard.connections.len(), 1);
    }

    #[test]
    fn group_masks_bound_unknown_endpoints_and_keep_address_families_separate() {
        assert!(IncomingGroupLimits::new(0, 24, 48).is_err());
        assert!(IncomingGroupLimits::new(32, 33, 48).is_err());
        assert!(IncomingGroupLimits::new(32, 24, 129).is_err());
        let mut guard = IncomingGroupGuard::new(IncomingGroupLimits::new(1, 24, 48).unwrap());
        let local = Multiaddr::empty();
        for (i, (first, same, separate)) in [
            (
                "/ip4/192.0.2.1/tcp/1",
                "/ip4/192.0.2.254/udp/2/quic-v1",
                "/ip4/192.0.3.1/tcp/1",
            ),
            (
                "/ip6/2001:db8:1::1/tcp/1",
                "/ip6/2001:db8:1:abcd::2/tcp/2",
                "/ip6/2001:db8:2::1/tcp/1",
            ),
            (
                "/dns/a.example/tcp/1",
                "/dns/b.example/tcp/2",
                "/ip4/198.51.100.1/tcp/1",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            guard
                .handle_pending_inbound_connection(
                    ConnectionId::new_unchecked(i * 3),
                    &local,
                    &first.parse().unwrap(),
                )
                .unwrap();
            assert!(guard
                .handle_pending_inbound_connection(
                    ConnectionId::new_unchecked(i * 3 + 1),
                    &local,
                    &same.parse().unwrap()
                )
                .is_err());
            guard
                .handle_pending_inbound_connection(
                    ConnectionId::new_unchecked(i * 3 + 2),
                    &local,
                    &separate.parse().unwrap(),
                )
                .unwrap();
        }
        let zero = IncomingGroupLimits::new(1, 0, 0).unwrap();
        assert_eq!(
            zero.group(&"/ip4/1.2.3.4/tcp/1".parse().unwrap()),
            zero.group(&"/ip4/9.8.7.6/tcp/2".parse().unwrap())
        );
        assert_ne!(
            zero.group(&"/ip4/1.2.3.4/tcp/1".parse().unwrap()),
            zero.group(&"/ip6/::ffff:1.2.3.4/tcp/1".parse().unwrap())
        );
        let full = IncomingGroupLimits::new(1, 32, 128).unwrap();
        assert_ne!(
            full.group(&"/ip6/::1/tcp/1".parse().unwrap()),
            full.group(&"/ip6/::2/tcp/1".parse().unwrap())
        );
        assert_ne!(
            full.group(&"/ip4/1.2.3.4/tcp/1".parse().unwrap()),
            full.group(&"/ip4/1.2.3.5/tcp/1".parse().unwrap())
        );
    }
}
