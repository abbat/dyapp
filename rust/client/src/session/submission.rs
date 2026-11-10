//! Submission retains the committed operation identity even when node delivery fails.

use super::{Error, Node, PreparedEnvelope, Session, VerifiedProfile};

/// A committed operation's immediate node-delivery result, not a peer receipt or read state.
#[derive(Debug)]
pub enum Submission {
    /// All required outbound writes were accepted by nodes.
    Accepted {
        /// Stable logical message or edit/delete operation ID.
        id: [u8; 16],
    },
    /// The operation is committed; its exact outbound writes remain available for retry.
    Queued {
        /// Stable logical message or edit/delete operation ID.
        id: [u8; 16],
        /// Immediate delivery failure; it does not undo the committed operation.
        error: Error,
    },
}

impl Submission {
    /// The committed message or operation identity, in either delivery state.
    #[must_use]
    pub fn id(&self) -> [u8; 16] {
        match self {
            Self::Accepted { id } | Self::Queued { id, .. } => *id,
        }
    }

    /// Require node acceptance, for callers whose contract is strict immediate delivery.
    ///
    /// # Errors
    /// Returns the queued delivery failure without staging another operation.
    pub fn into_accepted(self) -> Result<[u8; 16], Error> {
        match self {
            Self::Accepted { id } => Ok(id),
            Self::Queued { error, .. } => Err(error),
        }
    }
}

impl Session {
    /// Submit text to the other owner, retaining its ID on post-commit delivery failure.
    ///
    /// # Errors
    /// Returns profile selection, key-package or staging errors before a successful commit.
    pub async fn submit_text_to(
        &mut self,
        nodes: &mut [Node],
        profiles: &[VerifiedProfile],
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<Submission, Error> {
        let peer = self.peer_in(profiles)?;
        let packages = match self.pair_group(&peer) {
            Some(_) => Vec::new(),
            None => self.key_packages(nodes, profiles).await?,
        };
        let (id, writes) = self.stage_text_to(profiles, &packages, text, seal_key)?;
        Ok(self.submit_staged(nodes, id, writes, seal_key).await)
    }

    /// Submit an edit of an existing live own text, retaining the committed edit ID.
    ///
    /// # Errors
    /// Returns target, storage or MLS staging errors; absent/deleted targets never become texts.
    pub async fn submit_edit(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        target: [u8; 16],
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<Submission, Error> {
        let (id, writes) = self.stage_edit(group_id, target, text, seal_key)?;
        Ok(self.submit_staged(nodes, id, writes, seal_key).await)
    }

    /// Submit a terminal deletion of an existing live own message, retaining its operation ID.
    ///
    /// # Errors
    /// Returns target, storage or MLS staging errors before successful commitment.
    pub async fn submit_delete(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        target: [u8; 16],
        seal_key: &[u8; 32],
    ) -> Result<Submission, Error> {
        let (id, writes) = self.stage_delete(group_id, target, seal_key)?;
        Ok(self.submit_staged(nodes, id, writes, seal_key).await)
    }

    async fn submit_staged(
        &mut self,
        nodes: &mut [Node],
        id: [u8; 16],
        writes: Vec<PreparedEnvelope>,
        seal_key: &[u8; 32],
    ) -> Submission {
        match self.send_staged(nodes, writes, seal_key).await {
            Ok(()) => Submission::Accepted { id },
            Err(error) => Submission::Queued { id, error },
        }
    }
}

#[cfg(test)]
mod tests;
