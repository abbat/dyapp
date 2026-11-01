//! Forwarded text uses the same durable MLS and outbox staging as ordinary chat text.

use super::{Body, Error, Node, PreparedEnvelope, Session};

impl Session {
    /// Forward text as this MLS sender into an existing open conversation. The forwarded body
    /// names no original author; the new logical ID, HLC and exact encrypted writes are durable
    /// before any network request.
    pub fn stage_forwarded_text(
        &mut self,
        group_id: &[u8],
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<([u8; 16], Vec<PreparedEnvelope>), Error> {
        let peer = self.partner(group_id).ok_or(Error::UnknownGroup)?;
        if self.store.blocked(&peer)? {
            return Err(crate::store::Error::BlockedPeer.into());
        }
        if self.store.conversation(&peer)? != crate::store::Conversation::Open {
            return Err(Error::ConversationNotOpen);
        }
        let mut id = [0; 16];
        getrandom::fill(&mut id)
            .map_err(|_| Error::Client(crate::Error::Reply("no message randomness")))?;
        let body = Body::Forwarded(Box::new(Body::Text {
            id,
            hlc: Some(self.store.draft_chat_text(crate::now() * 1000)?),
            text: text.to_vec(),
        }));
        let writes = self.stage_message(group_id, &body, seal_key)?;
        Ok((id, writes))
    }

    /// Send a forwarded text after its exact writes are committed locally.
    pub async fn send_forwarded_text(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<[u8; 16], Error> {
        let (id, writes) = self.stage_forwarded_text(group_id, text, seal_key)?;
        self.send_staged(nodes, writes, seal_key).await?;
        Ok(id)
    }

    /// Forward into the pair's canonical joined group after the target profile was checked.
    pub async fn send_forwarded_text_to(
        &mut self,
        nodes: &mut [Node],
        peer_id: &str,
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<[u8; 16], Error> {
        let group_id = self.pair_group(peer_id).ok_or(Error::UnknownGroup)?;
        self.send_forwarded_text(nodes, &group_id, text, seal_key)
            .await
    }
}
