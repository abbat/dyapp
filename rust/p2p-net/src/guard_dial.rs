//! Outbound attempt lifetimes around a combined transport; listening and inbound are delegated.

use crate::{DialGuard, DialSlot};
use libp2p::core::transport::{DialOpts, ListenerId, TransportError, TransportEvent};
use libp2p::{Multiaddr, Transport};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

/// Wrap the combined QUIC/TCP transport once, below DNS, to charge resolved targets once.
pub struct GuardDial<T> {
    inner: T,
    guard: DialGuard,
}

impl<T> GuardDial<T> {
    /// Wraps `inner`, charging each resolved dial target to `guard`.
    pub fn new(inner: T, guard: DialGuard) -> Self {
        Self { inner, guard }
    }
}

/// Holds a dial slot until completion or cancellation, including before its first poll.
pub struct DialFuture<F> {
    inner: Pin<Box<F>>,
    slot: Option<DialSlot>,
}

impl<F, O, E> Future for DialFuture<F>
where
    F: Future<Output = Result<O, E>>,
{
    type Output = Result<O, E>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let result = match this.inner.as_mut().poll(context) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(result) => result,
        };
        if let Some(slot) = this.slot.take() {
            if result.is_err() {
                slot.fail(Instant::now());
            }
            // A success drops the slot without adding a failure.
        }
        Poll::Ready(result)
    }
}

impl<T: Transport + Unpin> Transport for GuardDial<T> {
    type Output = T::Output;
    type Error = T::Error;
    type ListenerUpgrade = T::ListenerUpgrade;
    type Dial = DialFuture<T::Dial>;

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
        let Some(slot) = self.guard.admit(&address, Instant::now()) else {
            return Err(TransportError::MultiaddrNotSupported(address));
        };
        match self.inner.dial(address, options) {
            Ok(inner) => Ok(DialFuture {
                inner: Box::pin(inner),
                slot: Some(slot),
            }),
            Err(error) => {
                slot.fail(Instant::now());
                Err(error)
            }
        }
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
    use libp2p::futures::{future, task::noop_waker};
    use std::collections::VecDeque;
    use std::future::{ready, Ready};
    use std::io;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Clone, Copy)]
    enum Outcome {
        Pending,
        Success,
        Failure,
        Immediate,
        Unsupported,
    }

    struct OnDrop(Arc<AtomicUsize>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    struct Spy {
        calls: Arc<[AtomicUsize; 4]>,
        drops: Arc<AtomicUsize>,
        outcome: Outcome,
        events: VecDeque<TransportEvent<Ready<io::Result<()>>, io::Error>>,
    }

    impl Spy {
        fn new(outcome: Outcome) -> Self {
            Self {
                calls: Arc::default(),
                drops: Arc::default(),
                outcome,
                events: VecDeque::new(),
            }
        }
    }

    impl Transport for Spy {
        type Output = ();
        type Error = io::Error;
        type ListenerUpgrade = Ready<io::Result<()>>;
        type Dial = Pin<Box<dyn Future<Output = io::Result<()>> + Send>>;

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
            address: Multiaddr,
            _: DialOpts,
        ) -> Result<Self::Dial, TransportError<Self::Error>> {
            self.calls[0].fetch_add(1, Ordering::Relaxed);
            match self.outcome {
                Outcome::Immediate => {
                    return Err(TransportError::Other(io::Error::other(
                        "immediate dial failure",
                    )))
                }
                Outcome::Unsupported => return Err(TransportError::MultiaddrNotSupported(address)),
                _ => {}
            }
            let (outcome, on_drop) = (self.outcome, OnDrop(self.drops.clone()));
            Ok(Box::pin(async move {
                let _on_drop = on_drop;
                match outcome {
                    Outcome::Pending => future::pending().await,
                    Outcome::Success => Ok(()),
                    Outcome::Failure => Err(io::Error::other("deferred dial failure")),
                    Outcome::Immediate | Outcome::Unsupported => unreachable!("returned above"),
                }
            }))
        }
        fn poll(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<TransportEvent<Self::ListenerUpgrade, Self::Error>> {
            let this = self.get_mut();
            this.calls[3].fetch_add(1, Ordering::Relaxed);
            this.events.pop_front().map_or(Poll::Pending, Poll::Ready)
        }
    }

    fn options() -> DialOpts {
        DialOpts {
            role: Endpoint::Dialer,
            port_use: PortUse::New,
        }
    }
    fn address() -> Multiaddr {
        "/ip4/9.1.2.3/tcp/1".parse().unwrap()
    }

    #[test]
    fn twenty_attempts_reach_inner_four_times_and_cancellation_frees_every_slot() {
        let guard = DialGuard::new(true);
        let mut transport = GuardDial::new(Spy::new(Outcome::Pending), guard.clone());
        let mut held = Vec::new();
        for i in 0..20 {
            let address = if i % 2 == 0 {
                format!("/ip4/9.1.2.{i}/tcp/1")
            } else {
                format!("/ip6/::ffff:9.1.2.{i}/udp/1/quic-v1")
            }
            .parse()
            .unwrap();
            if let Ok(future) = transport.dial(address, options()) {
                held.push(future);
            }
        }
        assert_eq!(held.len(), 4);
        assert_eq!(transport.inner.calls[0].load(Ordering::Relaxed), 4);
        assert_eq!(guard.take_denied(), 16);
        let waker = noop_waker();
        assert!(Pin::new(&mut held[0])
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
        drop(held); // Three unpolled and one polled pending future.
        assert_eq!(transport.inner.drops.load(Ordering::Relaxed), 4);
        let held = (0..4)
            .map(|_| transport.dial(address(), options()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(guard.take_denied(), 0);
        drop(held);
    }

    #[test]
    fn successful_completion_releases_before_future_drop_and_never_releases_twice() {
        let guard = DialGuard::new(true);
        let mut transport = GuardDial::new(Spy::new(Outcome::Success), guard.clone());
        let mut completed = transport.dial(address(), options()).unwrap();
        transport.inner.outcome = Outcome::Pending;
        let held = (0..3)
            .map(|_| transport.dial(address(), options()).unwrap())
            .collect::<Vec<_>>();
        assert!(transport.dial(address(), options()).is_err());
        let waker = noop_waker();
        assert!(matches!(
            Pin::new(&mut completed).poll(&mut Context::from_waker(&waker)),
            Poll::Ready(Ok(()))
        ));
        let replacement = transport.dial(address(), options()).unwrap();
        drop(completed);
        assert!(transport.dial(address(), options()).is_err());
        drop(replacement);
        drop(held);
        for _ in 0..20 {
            let future = transport.dial(address(), options()).unwrap();
            drop(future);
        }
        assert_eq!(guard.take_denied(), 2);
    }

    #[test]
    fn immediate_and_deferred_errors_count_once_and_stop_the_eleventh_inner_attempt() {
        for outcome in [Outcome::Immediate, Outcome::Unsupported, Outcome::Failure] {
            let guard = DialGuard::new(true);
            let mut transport = GuardDial::new(Spy::new(outcome), guard.clone());
            let waker = noop_waker();
            for _ in 0..10 {
                match transport.dial(address(), options()) {
                    Ok(mut future) => {
                        assert!(matches!(
                            Pin::new(&mut future).poll(&mut Context::from_waker(&waker)),
                            Poll::Ready(Err(_))
                        ));
                        drop(future);
                    }
                    Err(TransportError::Other(_) | TransportError::MultiaddrNotSupported(_)) => {}
                }
            }
            assert_eq!(transport.inner.calls[0].load(Ordering::Relaxed), 10);
            assert_eq!(guard.take_denied(), 0); // Actual errors are not policy refusals.
            assert!(transport.dial(address(), options()).is_err());
            assert_eq!(transport.inner.calls[0].load(Ordering::Relaxed), 10);
            assert_eq!(guard.take_denied(), 1);
        }
    }

    #[tokio::test]
    async fn rejected_ips_never_reach_inner_and_listeners_inbound_events_stay_unchanged() {
        let guard = DialGuard::new(true);
        let mut transport = GuardDial::new(Spy::new(Outcome::Success), guard.clone());
        let denied: Multiaddr = "/ip4/127.0.0.1/tcp/1".parse().unwrap();
        assert!(
            matches!(transport.dial(denied.clone(), options()), Err(TransportError::MultiaddrNotSupported(address)) if address == denied)
        );
        assert_eq!(transport.inner.calls[0].load(Ordering::Relaxed), 0);
        assert_eq!(guard.take_denied(), 1);
        let listener = ListenerId::next();
        transport.listen_on(listener, denied.clone()).unwrap();
        assert!(transport.remove_listener(listener));
        transport.inner.events.push_back(TransportEvent::Incoming {
            listener_id: listener,
            upgrade: ready(Ok(())),
            local_addr: denied.clone(),
            send_back_addr: denied.clone(),
        });
        let waker = noop_waker();
        match Pin::new(&mut transport).poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(TransportEvent::Incoming {
                listener_id,
                upgrade,
                local_addr,
                send_back_addr,
            }) => {
                assert_eq!(listener_id, listener);
                assert_eq!((local_addr, send_back_addr), (denied.clone(), denied));
                upgrade.await.unwrap();
            }
            _ => panic!("incoming event changed"),
        }
        assert!(Pin::new(&mut transport)
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
        assert_eq!(
            transport
                .inner
                .calls
                .each_ref()
                .map(|v| v.load(Ordering::Relaxed)),
            [0, 1, 1, 2]
        );
        assert_eq!(guard.take_denied(), 0);
    }

    #[test]
    fn disabled_group_policy_allows_private_immediate_failures_without_budgeting() {
        let guard = DialGuard::new(false);
        let mut transport = GuardDial::new(Spy::new(Outcome::Immediate), guard.clone());
        for _ in 0..20 {
            assert!(transport
                .dial("/ip4/127.0.0.1/tcp/1".parse().unwrap(), options())
                .is_err());
        }
        assert_eq!(transport.inner.calls[0].load(Ordering::Relaxed), 20);
        assert_eq!(guard.take_denied(), 0);
    }
}
