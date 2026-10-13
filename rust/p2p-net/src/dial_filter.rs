//! Outgoing IP filtering; runtime swarm/resolver integration is a separate step.

use ipnet::IpNet;
use libp2p::core::transport::{DialOpts, ListenerId, TransportError, TransportEvent};
use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, Transport};
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

/// Immutable outgoing deny ranges and a shared interval refusal counter. Default allows all.
#[derive(Clone, Default)]
pub struct DialFilter {
    nets: Arc<[IpNet]>,
    denied: Arc<AtomicU64>,
}

impl DialFilter {
    pub fn new(nets: Vec<IpNet>) -> Self {
        Self {
            nets: nets.into(),
            denied: Arc::default(),
        }
    }

    /// Check the first IP anywhere in the address; canonicalize IPv4-mapped IPv6.
    /// DNS-only addresses must be resolved before this predicate can filter them.
    pub fn denies(&self, address: &Multiaddr) -> bool {
        let Some(ip) = address.iter().find_map(|protocol| match protocol {
            Protocol::Ip4(ip) => Some(IpAddr::V4(ip)),
            Protocol::Ip6(ip) => Some(IpAddr::V6(ip)),
            _ => None,
        }) else {
            return false;
        };
        let canonical = match ip {
            IpAddr::V6(ip) => ip.to_canonical(),
            ip => ip,
        };
        self.nets
            .iter()
            .any(|net| net.contains(&ip) || net.contains(&canonical))
    }

    /// Refusals since the last call, shared by clones.
    pub fn take_denied(&self) -> u64 {
        self.denied.swap(0, Ordering::Relaxed)
    }
}

/// Deny outgoing IPs before calling the inner transport; listening and inbound are unchanged.
pub struct DenyDial<T> {
    inner: T,
    filter: DialFilter,
}

impl<T> DenyDial<T> {
    pub fn new(inner: T, filter: DialFilter) -> Self {
        Self { inner, filter }
    }
}

impl<T: Transport + Unpin> Transport for DenyDial<T> {
    type Output = T::Output;
    type Error = T::Error;
    type ListenerUpgrade = T::ListenerUpgrade;
    type Dial = T::Dial;

    fn listen_on(
        &mut self,
        id: ListenerId,
        address: Multiaddr,
    ) -> Result<(), TransportError<Self::Error>> {
        self.inner.listen_on(id, address)
    }

    fn remove_listener(&mut self, id: ListenerId) -> bool {
        self.inner.remove_listener(id)
    }

    fn dial(
        &mut self,
        address: Multiaddr,
        options: DialOpts,
    ) -> Result<Self::Dial, TransportError<Self::Error>> {
        if self.filter.denies(&address) {
            self.filter.denied.fetch_add(1, Ordering::Relaxed);
            return Err(TransportError::MultiaddrNotSupported(address));
        }
        self.inner.dial(address, options)
    }

    fn poll(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<TransportEvent<Self::ListenerUpgrade, Self::Error>> {
        Pin::new(&mut self.get_mut().inner).poll(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::core::transport::PortUse;
    use libp2p::core::Endpoint;
    use std::future::{ready, Ready};
    use std::io;
    use std::sync::atomic::AtomicUsize;

    fn policy() -> DialFilter {
        DialFilter::new(
            [
                "169.254.0.0/16",
                "fe80::/10",
                "fd00:ec2::254/128",
                "100.100.100.200/32",
            ]
            .iter()
            .map(|net| net.parse().unwrap())
            .collect(),
        )
    }

    #[test]
    fn addresses_cover_mapped_ipv6_zones_and_range_edges() {
        let filter = policy();
        for address in [
            "/ip4/169.254.0.0/tcp/80",
            "/ip4/169.254.255.255/tcp/80",
            "/ip4/169.254.169.254/tcp/80",
            "/ip6/fe80::1/tcp/80",
            "/ip6/febf::1/tcp/80",
            "/ip6/::ffff:169.254.169.254/tcp/80",
            "/ip6zone/eth0/ip6/fe80::1/tcp/80",
            "/ip4/100.100.100.200/tcp/80",
            "/ip6/fd00:ec2::254/tcp/80",
        ] {
            assert!(filter.denies(&address.parse().unwrap()), "{address}");
            assert!(!DialFilter::default().denies(&address.parse().unwrap()));
        }
        for address in [
            "/ip4/169.253.255.255/tcp/1",
            "/ip4/169.255.0.0/tcp/1",
            "/ip4/127.0.0.1/tcp/1",
            "/ip4/10.0.0.1/tcp/1",
            "/ip4/100.64.0.1/tcp/1",
            "/ip4/100.100.100.199/tcp/1",
            "/ip6/fec0::1/tcp/1",
            "/ip6/fd00:ec2::253/tcp/1",
            "/ip6/::ffff:127.0.0.1/tcp/1",
            "/dns4/example.org/tcp/1",
        ] {
            assert!(!filter.denies(&address.parse().unwrap()), "{address}");
        }
        let mapped = DialFilter::new(vec!["::ffff:169.254.0.0/112".parse().unwrap()]);
        assert!(mapped.denies(&"/ip6/::ffff:169.254.169.254/tcp/80".parse().unwrap()));
        // Pure checks do not count a refusal; only a rejected dial does.
        assert_eq!(filter.take_denied(), 0);
    }

    struct Spy {
        // dial, listen, remove, poll
        calls: Arc<[AtomicUsize; 4]>,
    }

    impl Transport for Spy {
        type Output = ();
        type Error = io::Error;
        type ListenerUpgrade = Ready<io::Result<()>>;
        type Dial = Ready<io::Result<()>>;

        fn listen_on(
            &mut self,
            _: ListenerId,
            _: Multiaddr,
        ) -> Result<(), TransportError<Self::Error>> {
            self.calls[1].fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn remove_listener(&mut self, _: ListenerId) -> bool {
            self.calls[2].fetch_add(1, Ordering::Relaxed);
            true
        }

        fn dial(
            &mut self,
            _: Multiaddr,
            _: DialOpts,
        ) -> Result<Self::Dial, TransportError<Self::Error>> {
            self.calls[0].fetch_add(1, Ordering::Relaxed);
            Ok(ready(Ok(())))
        }

        fn poll(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<TransportEvent<Self::ListenerUpgrade, Self::Error>> {
            self.calls[3].fetch_add(1, Ordering::Relaxed);
            Poll::Pending
        }
    }

    #[tokio::test]
    async fn rejected_dial_never_calls_inner_other_operations_delegate() {
        let calls: Arc<[AtomicUsize; 4]> = Arc::default();
        let filter = policy();
        let mut transport = DenyDial::new(
            Spy {
                calls: calls.clone(),
            },
            filter.clone(),
        );
        let options = || DialOpts {
            role: Endpoint::Dialer,
            port_use: PortUse::New,
        };
        let denied: Multiaddr = "/ip4/169.254.169.254/tcp/80".parse().unwrap();
        assert!(matches!(
            transport.dial(denied.clone(), options()),
            Err(TransportError::MultiaddrNotSupported(address)) if address == denied
        ));
        assert_eq!(calls[0].load(Ordering::Relaxed), 0);
        assert_eq!(filter.take_denied(), 1);
        assert_eq!(transport.filter.take_denied(), 0);
        transport
            .dial("/ip4/127.0.0.1/tcp/1".parse().unwrap(), options())
            .unwrap()
            .await
            .unwrap();
        let listener = ListenerId::next();
        transport.listen_on(listener, denied).unwrap();
        assert!(transport.remove_listener(listener));
        let waker = libp2p::futures::task::noop_waker();
        assert!(Pin::new(&mut transport)
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
        assert_eq!(
            calls.each_ref().map(|n| n.load(Ordering::Relaxed)),
            [1, 1, 1, 1]
        );
        assert_eq!(filter.take_denied(), 0);
    }
}
