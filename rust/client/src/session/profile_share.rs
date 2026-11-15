//! A profile link is ordinary text, admitted only into an existing open target pair.

use super::{Error, Node, Session, Submission, VerifiedProfile};
use crate::store::Conversation;
use dyapp_identity::profile_link::profile_link;

impl Session {
    /// Prepare a canonical profile link for a distinct, unblocked, open target pair.
    ///
    /// This read-only preflight may drive adapters before network work; submission always
    /// repeats admission on the same session that stages the text.
    ///
    /// # Errors
    /// Returns invalid target/link, blocked-peer or storage errors without staging anything.
    pub fn prepare_profile_share(&self, target: &str, shared: &str) -> Result<String, Error> {
        let text = profile_link(shared).ok_or(Error::InvalidProfileShare)?;
        if target == shared || crate::private_peer::check(&self.store, target).is_err() {
            return Err(Error::InvalidProfileShare);
        }
        if self.store.blocked(target)? {
            return Err(crate::store::Error::BlockedPeer.into());
        }
        if self.store.conversation(target)? != Conversation::Open
            || self.pair_group(target).is_none()
        {
            return Err(Error::InvalidProfileShare);
        }
        Ok(text)
    }

    /// Share into the verified profiles' existing open pair as ordinary, non-forwarded text.
    ///
    /// # Errors
    /// Returns preflight/profile/staging failures before commitment. Queued delivery retains
    /// the committed ID; a caller must not create a replacement submission.
    pub async fn submit_profile_share_to(
        &mut self,
        nodes: &mut [Node],
        profiles: &[VerifiedProfile],
        shared: &str,
        seal_key: &[u8; 32],
    ) -> Result<Submission, Error> {
        let target = self.peer_in(profiles)?;
        let text = self.prepare_profile_share(&target, shared)?;
        self.submit_text_to(nodes, profiles, text.as_bytes(), seal_key)
            .await
    }

    /// Share through an established pair when online profile retrieval is unavailable.
    ///
    /// # Errors
    /// Returns preflight/staging failures, never an invitation or an unrestricted text fallback.
    pub async fn submit_established_profile_share(
        &mut self,
        nodes: &mut [Node],
        target: &str,
        shared: &str,
        seal_key: &[u8; 32],
    ) -> Result<Submission, Error> {
        let text = self.prepare_profile_share(target, shared)?;
        self.try_submit_established_text(nodes, target, text.as_bytes(), seal_key)
            .await?
            .ok_or(Error::InvalidProfileShare)
    }
}
