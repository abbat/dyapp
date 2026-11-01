//! Clear chat: remove a conversation from this device only, keeping key-only marks so a
//! re-delivered message never shows again.

use super::MessageKey;
use crate::store::{Error, Store};
use rusqlite::{params, Transaction};

impl Store {
    /// Remove every current text and placeholder of `group`, its operations and its unsent
    /// outbox writes in one transaction, marking each removed message and each target of a
    /// removed pending operation. Returns how many texts and placeholders went. Nothing is sent.
    pub fn clear_chat(&mut self, group: &[u8]) -> Result<usize, Error> {
        if group.is_empty() {
            return Err(Error::InboxConflict);
        }
        let tx = self.db.transaction()?;
        let removed = clear_group_tx(&tx, group)?;
        tx.commit()?;
        Ok(removed)
    }

    /// Clear every cataloged MLS group for one peer in a single device-local transaction.
    /// Conversation state and other peers are unchanged; no receipt is sent.
    pub fn clear_chat_pair(&mut self, peer_id: &str) -> Result<usize, Error> {
        if peer_id.is_empty() || peer_id == self.owner_peer_id {
            return Err(Error::InboxConflict);
        }
        let tx = self.db.transaction()?;
        let groups = {
            let mut query =
                tx.prepare("SELECT `group_id` FROM `conversation_groups` WHERE `peer_id` = ?1")?;
            let rows = query.query_map([peer_id], |row| row.get::<_, Vec<u8>>(0))?;
            let groups = rows.collect::<Result<Vec<_>, _>>()?;
            groups
        };
        let mut removed = 0;
        for group in groups {
            removed += clear_group_tx(&tx, &group)?;
        }
        tx.commit()?;
        Ok(removed)
    }
}

fn clear_group_tx(tx: &Transaction<'_>, group: &[u8]) -> Result<usize, Error> {
    tx.execute(
        "INSERT OR IGNORE INTO `chat_cleared` (`group_id`, `author`, `message_id`)
         SELECT `group_id`, `author`, `message_id` FROM `chat_messages` WHERE `group_id` = ?1
         UNION SELECT `group_id`, `author`, `target_id` FROM `chat_operations`
         WHERE `group_id` = ?1 AND `pending` = 1",
        [group],
    )?;
    let removed = tx.execute("DELETE FROM `chat_messages` WHERE `group_id` = ?1", [group])?;
    tx.execute(
        "DELETE FROM `chat_operations` WHERE `group_id` = ?1",
        [group],
    )?;
    // A write a node already accepted is out; only the recipient can drop it now.
    tx.execute(
        "DELETE FROM `outbox` WHERE `chat_group` = ?1 AND `accepted_at_ms` IS NULL",
        [group],
    )?;
    Ok(removed)
}

/// Whether Clear chat removed `key`.
pub(super) fn is_cleared(tx: &Transaction<'_>, key: &MessageKey) -> Result<bool, Error> {
    Ok(tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM `chat_cleared`
         WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3)",
        params![&key.group, &key.author, key.id.as_slice()],
        |row| row.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::super::{Body, InboxKey, MessageKey, Store, VerifiedChatSender};
    use crate::prepare_put;
    use dyapp_identity::Identity;
    use dyapp_messaging::hlc::Timestamp;

    fn inbox(id: u8) -> InboxKey {
        InboxKey {
            mailbox: [1; 32],
            id: [id; 16],
        }
    }

    fn text(id: u8) -> Body {
        Body::Text {
            id: [id; 16],
            hlc: None,
            text: vec![id],
        }
    }

    #[test]
    fn cleared_messages_stay_gone_while_new_ones_and_other_groups_show() {
        let path = crate::store::tests::temp_db_path();
        let owner = Identity::generate();
        let (own, peer) = (owner.peer_id(), Identity::generate().peer_id());
        let (cleared, other, device) = ([3], [4], [5; 32]);
        let from = |group, author| VerifiedChatSender {
            group,
            author,
            device: &device,
        };
        let key = |group: &[u8], author: &str, id| MessageKey {
            group: group.to_vec(),
            author: author.to_owned(),
            id: [id; 16],
        };
        let mut store = Store::open(&path, &owner).unwrap();
        crate::store::tests::catalog_group(&mut store, &cleared, &peer);
        crate::store::tests::catalog_group(&mut store, &other, &peer);
        store
            .stage_chat_inbound(b"mls", &inbox(1), from(&cleared, &peer), &text(1), 1_000)
            .unwrap();
        let unsent = prepare_put(&[7; 32], vec![1]).unwrap();
        let (sent, plain) = (
            prepare_put(&[7; 32], vec![2]).unwrap(),
            prepare_put(&[7; 32], vec![3]).unwrap(),
        );
        store
            .stage_chat_outbound(b"mls", &[unsent], from(&cleared, &own), &text(2), 1_000)
            .unwrap();
        store
            .stage_chat_outbound(
                b"mls",
                std::slice::from_ref(&sent),
                from(&cleared, &own),
                &text(3),
                1_000,
            )
            .unwrap();
        store.accepted(&sent, 1_000, "node").unwrap();
        store.stage_outbound(b"mls", &[plain]).unwrap();
        // An edit whose original has not arrived yet.
        let early = Body::Edit {
            target: [8; 16],
            operation: [9; 16],
            hlc: Timestamp::new(100, 0).into(),
            text: b"early".to_vec(),
        };
        store
            .stage_chat_inbound(b"mls", &inbox(2), from(&cleared, &peer), &early, 1_000)
            .unwrap();
        store
            .stage_chat_inbound(b"mls", &inbox(3), from(&other, &peer), &text(1), 1_000)
            .unwrap();

        assert_eq!(store.clear_chat(&cleared).unwrap(), 3);
        for (author, id) in [(&peer, 1), (&own, 2), (&own, 3)] {
            assert!(store
                .chat_message(&key(&cleared, author, id))
                .unwrap()
                .is_none());
        }
        // The unsent text is cancelled; the accepted one and a non-chat write stay.
        assert_eq!(store.due_outbound(i64::MAX).unwrap().len(), 2);
        assert!(store
            .chat_message(&key(&other, &peer, 1))
            .unwrap()
            .is_some());
        drop(store);

        let mut store = Store::open(&path, &owner).unwrap();
        let edit = Body::Edit {
            target: [1; 16],
            operation: [10; 16],
            hlc: Timestamp::new(200, 0).into(),
            text: b"late".to_vec(),
        };
        let delete = Body::Delete {
            target: [1; 16],
            operation: [11; 16],
            hlc: Timestamp::new(300, 0).into(),
        };
        for (n, body) in [(4, text(1)), (5, edit), (6, delete), (7, text(8))] {
            assert!(!store
                .stage_chat_inbound(b"mls", &inbox(n), from(&cleared, &peer), &body, 1_000)
                .unwrap());
        }
        assert!(store
            .chat_message(&key(&cleared, &peer, 1))
            .unwrap()
            .is_none());
        assert!(store
            .chat_message(&key(&cleared, &peer, 8))
            .unwrap()
            .is_none());
        let acks = store.pending_acks().unwrap();
        assert!((4..=7).all(|n| acks.contains(&inbox(n))));
        store
            .stage_chat_inbound(b"mls", &inbox(9), from(&cleared, &peer), &text(12), 1_000)
            .unwrap();
        assert!(store
            .chat_message(&key(&cleared, &peer, 12))
            .unwrap()
            .is_some());
    }

    #[test]
    fn clearing_pair_removes_every_group_but_keeps_other_peer_and_replay_marks() {
        let path = crate::store::tests::temp_db_path();
        let owner = Identity::generate();
        let own = owner.peer_id();
        let peer = Identity::generate().peer_id();
        let other = Identity::generate().peer_id();
        let (first, second, unrelated, device) = ([3], [4], [5], [6; 32]);
        let from = |group, author| VerifiedChatSender {
            group,
            author,
            device: &device,
        };
        let mut store = Store::open(&path, &owner).unwrap();
        for (group, author) in [
            (&first[..], &peer),
            (&second[..], &peer),
            (&unrelated[..], &other),
        ] {
            crate::store::tests::catalog_group(&mut store, group, author);
        }
        store
            .stage_chat_inbound(b"mls", &inbox(1), from(&first, &peer), &text(1), 1_000)
            .unwrap();
        store
            .stage_chat_inbound(b"mls", &inbox(2), from(&first, &own), &text(2), 1_000)
            .unwrap();
        store
            .stage_chat_inbound(b"mls", &inbox(3), from(&second, &peer), &text(3), 1_000)
            .unwrap();
        store
            .stage_chat_inbound(b"mls", &inbox(4), from(&unrelated, &other), &text(4), 1_000)
            .unwrap();
        let pending = Body::Edit {
            target: [9; 16],
            operation: [10; 16],
            hlc: Timestamp::new(100, 0).into(),
            text: b"pending".to_vec(),
        };
        store
            .stage_chat_inbound(b"mls", &inbox(5), from(&second, &peer), &pending, 1_000)
            .unwrap();
        assert_eq!(store.clear_chat_pair(&peer).unwrap(), 3);
        assert_eq!(store.clear_chat_pair(&peer).unwrap(), 0);
        assert_eq!(
            store.conversation(&peer).unwrap(),
            crate::store::Conversation::Open
        );
        assert!(store
            .chat_message(&MessageKey {
                group: unrelated.to_vec(),
                author: other.clone(),
                id: [4; 16]
            })
            .unwrap()
            .is_some());
        drop(store);

        let mut store = Store::open(&path, &owner).unwrap();
        for (envelope, group, message) in [(6, &first[..], 1), (7, &second[..], 3)] {
            assert!(!store
                .stage_chat_inbound(
                    b"mls",
                    &inbox(envelope),
                    from(group, &peer),
                    &text(message),
                    1_000
                )
                .unwrap());
        }
        assert!(!store
            .stage_chat_inbound(b"mls", &inbox(8), from(&second, &peer), &text(9), 1_000)
            .unwrap());
        assert_eq!(store.mark_chat_read(&peer).unwrap(), 0);
        assert!(store
            .chat_message(&MessageKey {
                group: unrelated.to_vec(),
                author: other,
                id: [4; 16]
            })
            .unwrap()
            .is_some());
    }
}
