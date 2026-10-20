//! The IP each connection runs over, also when it was dialled through DNS.
//!
//! Below DNS a dialled connection's point is the resolved address that won. [`Tagged`] reports it
//! as the connection's first address change, before the swarm can poll any stream, and
//! [`Endpoints`] keeps the IP per connection. Kademlia then also swaps a routed DNS address for
//! the resolved one. QUIC connection migration (quic 0.14) is not reported: the IP stays the
//! handshake's.

use libp2p::core::muxing::{StreamMuxer, StreamMuxerBox, StreamMuxerEvent};
use libp2p::core::{transport::PortUse, ConnectedPoint, Endpoint};
use libp2p::multiaddr::Protocol;
use libp2p::swarm::{
    dummy, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::IpAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Boxes `muxer`, tagged with the address it was dialled at; a listener's is boxed as it is.
pub(crate) fn tag<M>(muxer: M, point: ConnectedPoint) -> StreamMuxerBox
where
    M: StreamMuxer + Send + Unpin + 'static,
    M::Substream: Send + 'static,
    M::Error: Send + Sync + 'static,
{
    match point {
        ConnectedPoint::Dialer { address, .. } => StreamMuxerBox::new(Tagged::new(muxer, address)),
        ConnectedPoint::Listener { .. } => StreamMuxerBox::new(muxer),
    }
}

/// Emits `AddressChange(address)` once, before the swarm can poll any stream, then delegates.
struct Tagged<M> {
    inner: M,
    announce: Option<Multiaddr>,
}

impl<M> Tagged<M> {
    fn new(inner: M, address: Multiaddr) -> Self {
        Self {
            inner,
            announce: Some(address),
        }
    }
}

impl<M: StreamMuxer + Unpin> StreamMuxer for Tagged<M> {
    type Substream = M::Substream;
    type Error = M::Error;

    fn poll_inbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        Pin::new(&mut self.get_mut().inner).poll_inbound(cx)
    }

    fn poll_outbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        Pin::new(&mut self.get_mut().inner).poll_outbound(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.get_mut().inner).poll_close(cx)
    }

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<StreamMuxerEvent, Self::Error>> {
        let this = self.get_mut();
        if let Some(address) = this.announce.take() {
            return Poll::Ready(Ok(StreamMuxerEvent::AddressChange(address)));
        }
        Pin::new(&mut this.inner).poll(cx)
    }
}

/// The canonical remote IP of each open connection: an inbound one's at once, a dialled one's
/// at once for an IP address and after its address change for a DNS name. Keyed by connection
/// only, never by peer or name.
#[derive(Default)]
pub struct Endpoints {
    ips: HashMap<ConnectionId, IpAddr>,
}

impl Endpoints {
    /// The remote IP of `connection`; `None` while it is unknown or the connection is closed.
    pub fn ip(&self, connection: ConnectionId) -> Option<IpAddr> {
        self.ips.get(&connection).copied()
    }

    /// Connections with a known IP.
    pub fn len(&self) -> usize {
        self.ips.len()
    }

    /// Whether no connection has a known IP.
    pub fn is_empty(&self) -> bool {
        self.ips.is_empty()
    }

    /// Records the IP `address` starts with; any other address forgets the connection's.
    fn record(&mut self, connection: ConnectionId, address: &Multiaddr) {
        let ip = match address.iter().next() {
            Some(Protocol::Ip4(ip)) => IpAddr::from(ip),
            Some(Protocol::Ip6(ip)) => IpAddr::from(ip).to_canonical(),
            _ => {
                self.ips.remove(&connection);
                return;
            }
        };
        self.ips.insert(connection, ip);
    }
}

impl NetworkBehaviour for Endpoints {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

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

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionEstablished(event) => {
                self.record(event.connection_id, event.endpoint.get_remote_address());
            }
            FromSwarm::AddressChange(event) => {
                self.record(event.connection_id, event.new.get_remote_address());
            }
            FromSwarm::ConnectionClosed(event) => {
                self.ips.remove(&event.connection_id);
            }
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
    use crate::{build_swarm, Behaviour, Mode};
    use libp2p::futures::StreamExt;
    use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
    use libp2p::swarm::SwarmEvent;
    use libp2p::{identity, Swarm};
    use std::time::Duration;

    const LOCAL: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);

    fn swarm() -> Swarm<Behaviour> {
        build_swarm(identity::Keypair::generate_ed25519(), Mode::Client).unwrap()
    }

    async fn listen(swarm: &mut Swarm<Behaviour>, address: &str) -> Multiaddr {
        swarm.listen_on(address.parse().unwrap()).unwrap();
        loop {
            if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
                return address;
            }
        }
    }

    /// Connections `a` saw open, close and fail to dial.
    #[derive(Default)]
    struct Seen {
        open: Vec<ConnectionId>,
        closed: Vec<ConnectionId>,
        failed: Vec<ConnectionId>,
    }

    /// Drives both swarms until `done` holds, for at most ten seconds.
    async fn until(
        a: &mut Swarm<Behaviour>,
        b: &mut Swarm<Behaviour>,
        seen: &mut Seen,
        done: impl Fn(&Swarm<Behaviour>, &Swarm<Behaviour>, &Seen) -> bool,
    ) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !done(a, b, seen) {
                tokio::select! {
                    event = a.select_next_some() => match event {
                        SwarmEvent::ConnectionEstablished { connection_id, .. } => {
                            seen.open.push(connection_id);
                        }
                        SwarmEvent::ConnectionClosed { connection_id, .. } => {
                            seen.closed.push(connection_id);
                        }
                        SwarmEvent::OutgoingConnectionError { connection_id, .. } => {
                            seen.failed.push(connection_id);
                        }
                        _ => {}
                    },
                    _ = b.select_next_some() => {}
                    // An address change raises no swarm event: look again now and then.
                    () = tokio::time::sleep(Duration::from_millis(20)) => {}
                }
            }
        })
        .await
        .unwrap();
    }

    fn dial(swarm: &mut Swarm<Behaviour>, peer: PeerId, addresses: Vec<Multiaddr>) -> ConnectionId {
        let dial = DialOpts::peer_id(peer)
            .addresses(addresses)
            .condition(PeerCondition::Always)
            .build();
        let id = dial.connection_id();
        swarm.dial(dial).unwrap();
        id
    }

    fn port(address: &Multiaddr) -> u16 {
        address
            .iter()
            .find_map(|p| match p {
                Protocol::Tcp(port) => Some(port),
                _ => None,
            })
            .unwrap()
    }

    #[tokio::test]
    async fn dns_and_direct_dials_record_the_ip_that_answered() {
        let (mut a, mut b) = (swarm(), swarm());
        let direct = listen(&mut b, "/ip4/127.0.0.1/tcp/0").await;
        let peer = *b.local_peer_id();
        let name: Multiaddr = format!("/dns4/localhost/tcp/{}", port(&direct))
            .parse()
            .unwrap();
        // Nothing listens on the first address: the race leaves only the live IP.
        let dead: Multiaddr = "/ip4/127.0.0.2/tcp/1".parse().unwrap();
        let dials = [
            dial(&mut a, peer, vec![dead.clone(), name.clone()]),
            dial(&mut a, peer, vec![dead, name]),
            dial(&mut a, peer, vec![direct]),
        ];
        let mut seen = Seen::default();
        until(&mut a, &mut b, &mut seen, |a, b, _| {
            let endpoints = &a.behaviour().endpoints;
            dials.iter().all(|id| endpoints.ip(*id) == Some(LOCAL))
                && b.behaviour().endpoints.len() == 3
        })
        .await;
        assert_eq!(a.behaviour().endpoints.len(), 3);
        assert!(seen.open.iter().all(|id| dials.contains(id)));
    }

    #[tokio::test]
    async fn quic_records_both_sides_and_failures_and_closes_leave_nothing() {
        let (mut a, mut b) = (swarm(), swarm());
        let address = listen(&mut b, "/ip4/127.0.0.1/udp/0/quic-v1").await;
        let peer = *b.local_peer_id();
        let live = dial(&mut a, peer, vec![address]);
        let dead = dial(
            &mut a,
            PeerId::random(),
            vec!["/ip4/127.0.0.2/tcp/1".parse().unwrap()],
        );
        let mut seen = Seen::default();
        until(&mut a, &mut b, &mut seen, |a, b, seen| {
            a.behaviour().endpoints.ip(live) == Some(LOCAL)
                && b.behaviour().endpoints.len() == 1
                && seen.failed.contains(&dead)
        })
        .await;
        assert_eq!(a.behaviour().endpoints.ip(dead), None);
        assert_eq!(a.behaviour().endpoints.len(), 1);

        a.disconnect_peer_id(peer).unwrap();
        until(&mut a, &mut b, &mut seen, |_, b, seen| {
            seen.closed.contains(&live) && b.behaviour().endpoints.is_empty()
        })
        .await;
        assert!(a.behaviour().endpoints.is_empty());
    }

    #[test]
    fn an_ipv4_mapped_address_is_recorded_as_ipv4() {
        let mut endpoints = Endpoints::default();
        let id = ConnectionId::new_unchecked(1);
        endpoints.record(id, &"/ip6/::ffff:192.0.2.1/tcp/1".parse().unwrap());
        assert_eq!(endpoints.ip(id), Some("192.0.2.1".parse().unwrap()));
        endpoints.record(id, &"/dns4/example.com/tcp/1".parse().unwrap());
        assert_eq!(endpoints.ip(id), None);
    }
}
