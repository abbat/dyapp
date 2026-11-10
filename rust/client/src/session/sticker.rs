//! Sticker metadata sends reuse the chat outbox; uploading bytes is a separate step.
mod download;
mod exchange;
#[cfg(test)]
mod tests;
mod transfer;
mod upload;
use super::{Body, Error, Node, PreparedEnvelope, Session};
use crate::chat::sticker::Sticker;

impl Session {
    /// Stage a sticker into an open pair chat with a fresh logical ID and sender HLC.
    /// The template's ID/HLC are ignored; its fallback labels and optional descriptor are validated.
    ///
    /// # Errors
    ///
    /// Returns an error if the group is unknown, the peer is blocked, the chat is
    /// not open, randomness or clock access fails, sticker validation fails,
    /// or MLS/outbox staging fails.
    pub fn stage_sticker(
        &mut self,
        group_id: &[u8],
        template: &Sticker,
        seal_key: &[u8; 32],
    ) -> Result<([u8; 16], Vec<PreparedEnvelope>), Error> {
        let peer = self.partner(group_id).ok_or(Error::UnknownGroup)?;
        if self.store.blocked(&peer)? {
            return Err(crate::store::Error::BlockedPeer.into());
        }
        if self.store.conversation(&peer)? != crate::store::Conversation::Open {
            return Err(Error::ConversationNotOpen);
        }
        let mut sticker = template.clone();
        getrandom::fill(&mut sticker.id)
            .map_err(|_| Error::Client(crate::Error::Reply("no message randomness")))?;
        sticker.hlc = self.store.draft_chat_text(crate::now() * 1000)?;
        sticker.encode()?;
        let id = sticker.id;
        let writes = self.stage_message(group_id, &Body::Sticker(Box::new(sticker)), seal_key)?;
        Ok((id, writes))
    }

    /// Deliver only after the exact encrypted writes and local projection are durable.
    ///
    /// # Errors
    ///
    /// Returns an error if [`Self::stage_sticker`] or staged delivery fails.
    /// Committed writes remain available for retry.
    pub async fn send_sticker(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        template: &Sticker,
        seal_key: &[u8; 32],
    ) -> Result<[u8; 16], Error> {
        self.submit_sticker(nodes, group_id, template, seal_key)
            .await?
            .into_accepted()
    }

    /// Send through the pair's canonical joined group; never opens a request.
    ///
    /// # Errors
    ///
    /// Returns an error if the pair has no joined group or [`Self::send_sticker`] fails.
    pub async fn send_sticker_to(
        &mut self,
        nodes: &mut [Node],
        peer_id: &str,
        template: &Sticker,
        seal_key: &[u8; 32],
    ) -> Result<[u8; 16], Error> {
        self.submit_sticker_to(nodes, peer_id, template, seal_key)
            .await?
            .into_accepted()
    }
}
