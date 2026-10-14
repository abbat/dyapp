//! Prototype: a muxer below DNS reports the dialled IP as the connection's first address change.

use libp2p::core::muxing::{StreamMuxer, StreamMuxerEvent};
use libp2p::Multiaddr;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Emits `AddressChange(address)` once, before the swarm can poll any stream, then delegates.
pub struct Tagged<M> {
    inner: M,
    announce: Option<Multiaddr>,
}

impl<M> Tagged<M> {
    pub fn new(inner: M, address: Multiaddr) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::core::muxing::StreamMuxerBox;
    use libp2p::core::transport::{Boxed, PortUse};
    use libp2p::core::upgrade::Version;
    use libp2p::core::{ConnectedPoint, Endpoint};
    use libp2p::futures::StreamExt;
    use libp2p::swarm::dial_opts::DialOpts;
    use libp2p::swarm::{
        dummy, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, SwarmEvent, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm,
    };
    use libp2p::{dns, identity, noise, tcp, yamux, PeerId, Swarm, Transport};
    use std::time::Duration;

    /// Records the remote address the swarm reports per connection; the counter wakes the
    /// test, since an address change produces no `SwarmEvent` of its own.
    #[derive(Default)]
    struct Log(Vec<(ConnectionId, &'static str, Multiaddr)>, usize);

    impl NetworkBehaviour for Log {
        type ConnectionHandler = dummy::ConnectionHandler;
        type ToSwarm = ();

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
            id: ConnectionId,
            _: PeerId,
            address: &Multiaddr,
            _: Endpoint,
            _: PortUse,
        ) -> Result<THandler<Self>, ConnectionDenied> {
            self.0.push((id, "established", address.clone()));
            Ok(dummy::ConnectionHandler)
        }

        fn on_swarm_event(&mut self, event: FromSwarm) {
            if let FromSwarm::AddressChange(change) = event {
                self.0.push((
                    change.connection_id,
                    "changed",
                    change.new.get_remote_address().clone(),
                ));
                self.1 += 1;
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

        fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<(), THandlerInEvent<Self>>> {
            if self.1 == 0 {
                return Poll::Pending;
            }
            self.1 -= 1;
            Poll::Ready(ToSwarm::GenerateEvent(()))
        }
    }

    fn swarm() -> Swarm<Log> {
        let key = identity::Keypair::generate_ed25519();
        let tcp = tcp::tokio::Transport::new(tcp::Config::default())
            .upgrade(Version::V1)
            .authenticate(noise::Config::new(&key).unwrap())
            .multiplex(yamux::Config::default())
            .map(|(peer, muxer), point| match point {
                // Below DNS, a dialer's point is the resolved address that won.
                ConnectedPoint::Dialer { address, .. } => {
                    (peer, StreamMuxerBox::new(Tagged::new(muxer, address)))
                }
                ConnectedPoint::Listener { .. } => (peer, StreamMuxerBox::new(muxer)),
            });
        let transport: Boxed<(PeerId, StreamMuxerBox)> =
            dns::tokio::Transport::system(tcp).unwrap().boxed();
        Swarm::new(
            transport,
            Log::default(),
            key.public().to_peer_id(),
            libp2p::swarm::Config::with_tokio_executor()
                .with_idle_connection_timeout(Duration::from_secs(30)),
        )
    }

    #[tokio::test]
    async fn dns_dial_reports_the_winning_ip_for_its_own_connection() {
        let mut listener = swarm();
        listener
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let port = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = listener.select_next_some().await {
                break address
                    .iter()
                    .find_map(|p| match p {
                        libp2p::multiaddr::Protocol::Tcp(port) => Some(port),
                        _ => None,
                    })
                    .unwrap();
            }
        };
        let peer = *listener.local_peer_id();
        tokio::spawn(async move { while listener.next().await.is_some() {} });

        let mut dialer = swarm();
        // Two concurrent dials to the same peer through one hostname; the first port is closed.
        let dead: Multiaddr = "/dns4/localhost/tcp/1".parse().unwrap();
        let live: Multiaddr = format!("/dns4/localhost/tcp/{port}").parse().unwrap();
        for _ in 0..2 {
            dialer
                .dial(
                    DialOpts::peer_id(peer)
                        .addresses(vec![dead.clone(), live.clone()])
                        .condition(libp2p::swarm::dial_opts::PeerCondition::Always)
                        .build(),
                )
                .unwrap();
        }
        // The swarm appends `/p2p/<peer>` before dialling; DNS keeps it on the resolved address.
        let live = live.with_p2p(peer).unwrap();
        let ip: Multiaddr = format!("/ip4/127.0.0.1/tcp/{port}/p2p/{peer}")
            .parse()
            .unwrap();
        let changed = |log: &Log| log.0.iter().filter(|e| e.1 == "changed").count();
        tokio::time::timeout(Duration::from_secs(10), async {
            while changed(dialer.behaviour()) < 2 {
                dialer.select_next_some().await;
            }
        })
        .await
        .unwrap();
        let log = &dialer.behaviour().0;
        let established: Vec<_> = log.iter().filter(|e| e.1 == "established").collect();
        assert_eq!(established.len(), 2);
        for (id, _, address) in established {
            assert_eq!(address, &live); // What swarm 0.48 reports without the tag.
            let changes: Vec<_> = log
                .iter()
                .filter(|e| e.0 == *id && e.1 == "changed")
                .collect();
            assert_eq!(changes.len(), 1, "exactly one change per connection");
            assert_eq!(changes[0].2, ip);
        }
    }
}
