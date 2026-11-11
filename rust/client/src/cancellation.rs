//! Per-attempt cancellation that remains observable before a worker starts or subscribes.

use tokio::sync::watch;

/// One attempt's cancellation owner. Allocate a new owner for each retry.
#[derive(Debug)]
pub struct Cancellation {
    state: watch::Sender<bool>,
}

impl Default for Cancellation {
    fn default() -> Self {
        let (state, _) = watch::channel(false);
        Self { state }
    }
}

impl Cancellation {
    /// Create an independently cancellable attempt without starting any work.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancel this attempt, including work that has not subscribed yet.
    pub fn cancel(&self) {
        self.state.send_replace(true);
    }

    /// Whether cancellation has already been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        *self.state.borrow()
    }

    /// Wait until cancellation; subscribing after cancellation completes immediately.
    pub async fn cancelled(&self) {
        let mut state = self.state.subscribe();
        let _ = state.wait_for(|cancelled| *cancelled).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::{pin, Pin};
    use std::task::{Context, Poll, Waker};

    fn poll(future: Pin<&mut impl Future<Output = ()>>) -> Poll<()> {
        future.poll(&mut Context::from_waker(Waker::noop()))
    }

    #[test]
    fn cancellation_before_subscription_is_retained() {
        let attempt = Cancellation::new();
        attempt.cancel();
        assert!(attempt.is_cancelled());
        assert_eq!(poll(pin!(attempt.cancelled())), Poll::Ready(()));
    }

    #[test]
    fn registered_wait_observes_cancellation_but_a_retry_does_not_inherit_it() {
        let attempt = Cancellation::new();
        let mut waiting = pin!(attempt.cancelled());
        assert_eq!(poll(waiting.as_mut()), Poll::Pending);
        attempt.cancel();
        assert_eq!(poll(waiting.as_mut()), Poll::Ready(()));
        let retry = Cancellation::new();
        let mut next = pin!(retry.cancelled());
        assert!(!retry.is_cancelled());
        attempt.cancel();
        assert_eq!(poll(next.as_mut()), Poll::Pending);
        retry.cancel();
        assert_eq!(poll(next.as_mut()), Poll::Ready(()));
    }
}
