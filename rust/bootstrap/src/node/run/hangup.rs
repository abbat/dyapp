//! Unix operator reload signal; other platforms retain timer-based deny-list refresh.

pub(super) struct Hangup {
    #[cfg(unix)]
    signal: tokio::signal::unix::Signal,
}

impl Hangup {
    pub(super) fn new() -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            Ok(Self {
                signal: signal(SignalKind::hangup())?,
            })
        }
        #[cfg(not(unix))]
        Ok(Self {})
    }

    pub(super) async fn recv(&mut self) {
        #[cfg(unix)]
        let _ = self.signal.recv().await;
        #[cfg(not(unix))]
        std::future::pending::<()>().await;
    }
}
