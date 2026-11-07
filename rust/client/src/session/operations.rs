//! Stage edits/deletions through the same durable MLS and chat projection path.
use super::{Body, Error, PreparedEnvelope, Session};
use crate::store::messages::MessageKey;

impl Session {
    pub(super) fn stage_chat_operation(
        &mut self,
        group_id: &[u8],
        target: [u8; 16],
        replacement: Option<&[u8]>,
        seal_key: &[u8; 32],
    ) -> Result<([u8; 16], Vec<PreparedEnvelope>), Error> {
        let key = MessageKey {
            group: group_id.to_vec(),
            author: self.store.owner_peer_id().to_owned(),
            id: target,
        };
        match self.store.chat_message(&key)? {
            Some(current) if !current.deleted => {}
            None if replacement.is_none() && self.store.has_chat_sticker(&key)? => {}
            _ => return Err(Error::UnknownMessage),
        }
        let (operation, hlc) = self.store.draft_chat_operation(crate::now() * 1000)?;
        let body = match replacement {
            Some(text) => Body::Edit {
                target,
                operation,
                hlc,
                text: text.to_vec(),
            },
            None => Body::Delete {
                target,
                operation,
                hlc,
            },
        };
        let writes = self.stage_message(group_id, &body, seal_key)?;
        Ok((operation, writes))
    }
}
