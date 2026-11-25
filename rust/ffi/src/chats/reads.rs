//! Opt-in read receipts of one shown chat, the same rules as clients/desktop/chats/reads.rs; see
//! docs/design/read-receipts.md.

use super::{failed, ChatMessage, ChatSession};
use crate::profile_network::{active_network, connect_at, on_network_stack};
use crate::CoreError;
use dyapp_client::session::Session;
use dyapp_client::store::Store;

/// Apply the core's persisted-arrival read snapshot to native display records.
pub(super) fn project_reads(
    store: &Store,
    peer_id: &str,
    messages: &mut [ChatMessage],
) -> Result<(), CoreError> {
    let coverage = store.read_coverage(peer_id).map_err(failed)?;
    for message in messages {
        if let Some(read) = coverage.read_flag(
            &message.group_id,
            &message.author,
            &message.id,
            message.edited,
            message.deleted,
        ) {
            message.read = read;
        }
    }
    Ok(())
}

#[uniffi::export]
impl ChatSession {
    /// Whether this profile sends read receipts; off by default.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock or stored receipt-setting lookup fails.
    pub fn read_receipts(&self) -> Result<bool, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session.store().send_read_receipts().map_err(failed)
    }

    /// Turn read receipts on or off; off stops new ones, sent ones stay with the peer.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock or storing the receipt setting fails.
    pub fn set_read_receipts(&self, enabled: bool) -> Result<(), CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session
            .store()
            .set_send_read_receipts(enabled)
            .map_err(failed)
    }

    /// Send the receipts the shown chat with `peer_id` owes and return how many went out. With
    /// nothing due, always the case while receipts are off, this never touches the network.
    /// Errors as in [`Self::clear_chat`], then `notConnected` or a node or storage message.
    ///
    /// # Errors
    ///
    /// Returns an error for the own, blocked or unknown peer, or if session locking, stored
    /// read-state lookup, network access, receipt sending or result-count conversion fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn send_reads(&self, peer_id: String) -> Result<u32, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        Self::check_visible(session, &peer_id)?;
        let due = session
            .store()
            .due_read_receipts(&peer_id)
            .map_err(failed)?;
        if due.is_empty() {
            return Ok(0);
        }
        let network = active_network(&self.dir)?;
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect_at(&self.dir, &network).await?;
                for (group, seen) in &due {
                    let nodes = std::slice::from_mut(&mut node);
                    session
                        .mark_read(nodes, group, *seen, &self.seal_key)
                        .await
                        .map_err(failed)?;
                }
                u32::try_from(due.len()).map_err(failed)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(group: u8, author: &str, id: u8) -> ChatMessage {
        ChatMessage {
            group_id: vec![group],
            author: author.into(),
            id: vec![id; 16],
            text: Some("text".into()),
            edited: false,
            deleted: false,
            unread: false,
            forwarded: false,
            delivery: None,
            read: false,
            sent_at_ms: None,
        }
    }

    #[test]
    fn a_mark_covers_earlier_live_own_texts_of_its_group_only() {
        let dir = dyapp_test_support::test_dir().unwrap();
        let owner = crate::IdentityKey::generate().unwrap();
        let own = owner.peer_id();
        let peer = crate::IdentityKey::generate().unwrap().peer_id();
        let mut store = Store::open(&dir.path().join("reads.db"), &owner.identity).unwrap();
        let mut messages = vec![
            message(1, &own, 1),
            message(1, &peer, 2),
            message(1, &own, 3),
            message(1, &own, 4),
            message(2, &own, 5),
        ];
        for message in &messages {
            let key = dyapp_client::store::messages::MessageKey {
                group: message.group_id.clone(),
                author: message.author.clone(),
                id: message.id.as_slice().try_into().unwrap(),
            };
            assert!(store.record_chat_text(&key, b"text").unwrap());
        }
        assert!(store.receive_read(&[1], &peer, [3; 16]).unwrap());
        messages[0].edited = true;
        project_reads(&store, &peer, &mut messages).unwrap();
        let read: Vec<_> = messages.iter().map(|message| message.read).collect();
        assert_eq!(read, [false, false, true, false, false]);
    }

    #[test]
    fn a_request_and_a_chat_without_texts_owe_nothing() {
        use dyapp_client::interactions::{Interaction, Kind};
        use dyapp_client::store::Conversation;
        let dir = dyapp_test_support::test_dir().unwrap();
        let own = crate::IdentityKey::generate().unwrap();
        let (own_id, peer_id) = (
            own.peer_id(),
            crate::IdentityKey::generate().unwrap().peer_id(),
        );
        let mut store = Store::open(&dir.path().join("client.db"), &own.identity).unwrap();
        assert_eq!(store.due_read_receipts(&peer_id).unwrap(), []);
        // Mutual likes open the conversation, which still has no partner text to receipt.
        for (sender, target, event_id) in [(&own_id, &peer_id, 1), (&peer_id, &own_id, 2)] {
            let interaction = Interaction {
                kind: Kind::Like,
                event_id: [event_id; 16],
                hlc: u64::from(event_id) << 16,
            };
            store
                .apply_interaction(sender, target, &[event_id; 32], interaction)
                .unwrap();
        }
        assert_eq!(store.conversation(&peer_id).unwrap(), Conversation::Open);
        assert_eq!(store.due_read_receipts(&peer_id).unwrap(), []);
    }
}
