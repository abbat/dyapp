//! Opt-in read receipts of one shown chat, the same rules as clients/desktop/chats/reads.rs; see
//! docs/design/read-receipts.md.

use super::{failed, ChatMessage, ChatSession};
use crate::profile_network::{active_network, connect, on_network_stack};
use crate::CoreError;
use dyapp_client::session::Session;
use dyapp_client::store::{Conversation, Store};
use std::collections::{BTreeMap, HashMap, HashSet};

/// A group ID and the latest partner text ID in it.
type Due = (Vec<u8>, [u8; 16]);

/// The latest partner text of each group that still owes a receipt. A request owes none: only an
/// accepted conversation may learn when the chat was opened.
fn due_reads(store: &Store, peer_id: &str) -> Result<Vec<Due>, CoreError> {
    if store.conversation(peer_id).map_err(failed)? != Conversation::Open {
        return Ok(Vec::new());
    }
    let latest: BTreeMap<_, _> = store
        .current_chat_messages(peer_id)
        .map_err(failed)?
        .into_iter()
        .filter(|message| message.key.author == peer_id)
        .map(|message| (message.key.group, message.key.id))
        .collect();
    let mut due = Vec::new();
    for (group, id) in latest {
        if store.draft_read(&group, peer_id, id).map_err(failed)? {
            due.push((group, id));
        }
    }
    Ok(due)
}

/// Set `read` on own messages up to the peer's read mark of their group; see [`covered`].
pub(super) fn project_reads(
    store: &Store,
    peer_id: &str,
    own: &str,
    messages: &mut [ChatMessage],
) -> Result<(), CoreError> {
    let mut marks = HashMap::new();
    for message in messages.iter() {
        if !marks.contains_key(&message.group_id) {
            let mark = store
                .read_up_to(&message.group_id, peer_id)
                .map_err(failed)?;
            marks.insert(message.group_id.clone(), mark.map(|id| id.to_vec()));
        }
    }
    covered(&marks, own, messages);
    Ok(())
}

/// A mark covers its text and every earlier own one of the group. A mark whose text was cleared
/// here marks nothing, and it says nothing about an edit made after reading or a deleted text.
fn covered(marks: &HashMap<Vec<u8>, Option<Vec<u8>>>, own: &str, messages: &mut [ChatMessage]) {
    let mut reached = HashSet::new();
    for message in messages.iter_mut().rev().filter(|m| m.author == own) {
        if marks[&message.group_id].as_ref() == Some(&message.id) {
            reached.insert(message.group_id.clone());
        }
        message.read = reached.contains(&message.group_id) && !message.edited && !message.deleted;
    }
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
    pub fn send_reads(&self, peer_id: String) -> Result<u32, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        // The guard stays on this thread; the session it lends moves to the network thread.
        let session: &mut Session = &mut guard;
        self.check_visible(session, &peer_id)?;
        let due = due_reads(session.store(), &peer_id)?;
        if due.is_empty() {
            return Ok(0);
        }
        let network = active_network(&self.dir)?;
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
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
        let mut messages = vec![
            message(1, "me", 1),
            message(1, "peer", 2),
            message(1, "me", 3),
            message(1, "me", 4),
            message(2, "me", 5),
        ];
        messages[0].edited = true;
        let marks = HashMap::from([(vec![1], Some(vec![3; 16])), (vec![2], None)]);
        covered(&marks, "me", &mut messages);
        let read: Vec<_> = messages.iter().map(|message| message.read).collect();
        assert_eq!(read, [false, false, true, false, false]);
    }

    #[test]
    fn a_request_and_a_chat_without_texts_owe_nothing() {
        use dyapp_client::interactions::{Interaction, Kind};
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-ffi-reads-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let own = crate::IdentityKey::generate();
        let (own_id, peer_id) = (own.peer_id(), crate::IdentityKey::generate().peer_id());
        let mut store = Store::open(&dir.join("client.db"), &own.identity).unwrap();
        assert_eq!(
            due_reads(&store, &peer_id).unwrap(),
            [] as [(std::vec::Vec<u8>, [u8; 16]); 0]
        );
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
        assert_eq!(
            due_reads(&store, &peer_id).unwrap(),
            [] as [(std::vec::Vec<u8>, [u8; 16]); 0]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
