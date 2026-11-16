//! Preserve the distinction between confirmed mailbox revocation and an original failure.
use super::{Error, Session};
use crate::Node;
use dyapp_p2p_net::proto::Status;

#[cfg(test)]
mod server;
#[cfg(test)]
mod tests;

/// Classification of one actual mailbox failure; never inferred from error text.
#[derive(Debug)]
pub enum MailboxFailure {
    /// A refused mailbox and verified newer ordinary profile jointly revoke this existing device.
    ConfirmedRevocation,
    /// The unchanged original failure, including an already supplied `Error::Revoked`.
    Original(Error),
}

impl Session {
    /// Classify an actual mailbox failure before a platform creates any cleanup capability.
    ///
    /// Refusal alone is insufficient. Original revocation errors are preserved as original errors,
    /// so callers cannot confuse their variant or message with this call's verified two-factor result.
    /// This never deletes data or authorizes cleanup of another local device context.
    pub async fn classify_mailbox_failure(&self, node: &mut Node, error: Error) -> MailboxFailure {
        if !matches!(error, Error::Client(crate::Error::Status(Status::Refused))) {
            return MailboxFailure::Original(error);
        }
        match node.get_profile(self.store.owner_peer_id()).await {
            Ok(newest) if matches!(self.own_device_revoked(&newest.profile), Ok(true)) => {
                MailboxFailure::ConfirmedRevocation
            }
            _ => MailboxFailure::Original(error),
        }
    }
}
