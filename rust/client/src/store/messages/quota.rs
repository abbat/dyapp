//! Per-contact visible-history bound, applied inside each projection transaction.

use super::{Error, MessageKey};
use rusqlite::{params, OptionalExtension, Transaction};

pub(super) const MAX_CONTACT_ITEMS: i64 = 10_000;
const REPLAY_RETENTION_MS: i64 = 31 * 24 * 60 * 60 * 1000;

pub(in crate::store) fn now_ms() -> Result<i64, Error> {
    i64::try_from(crate::now().saturating_mul(1000)).map_err(|_| Error::ClockRange)
}

pub(super) fn enforce(tx: &Transaction<'_>, key: &MessageKey) -> Result<bool, Error> {
    let peer: Option<String> = tx
        .query_row(
            "SELECT `peer_id` FROM `conversation_groups` WHERE `group_id` = ?1",
            [&key.group],
            |row| row.get(0),
        )
        .optional()?;
    let Some(peer) = peer else {
        return Ok(true);
    };
    let count: i64 = tx.query_row(
        "SELECT (SELECT COUNT(*) FROM `chat_messages` WHERE `group_id` IN
             (SELECT `group_id` FROM `conversation_groups` WHERE `peer_id` = ?1))
             + (SELECT COUNT(*) FROM `chat_attachments` WHERE `group_id` IN
             (SELECT `group_id` FROM `conversation_groups` WHERE `peer_id` = ?1))",
        [&peer],
        |row| row.get(0),
    )?;
    for _ in MAX_CONTACT_ITEMS..count {
        evict_oldest(tx, &peer, now_ms()?)?;
    }
    super::clear::is_cleared(tx, key).map(|cleared| !cleared)
}

fn evict_oldest(tx: &Transaction<'_>, peer: &str, now: i64) -> Result<(), Error> {
    let selected: Option<(i64, Vec<u8>, String, Vec<u8>)> = tx
        .query_row(
            "SELECT `kind`, `group_id`, `author`, `message_id` FROM (
                 SELECT 1 AS `kind`, `group_id`, `author`, `message_id`,
                     COALESCE(`original_hlc`, 0) AS `sort_hlc`
                 FROM `chat_messages` WHERE `group_id` IN
                     (SELECT `group_id` FROM `conversation_groups` WHERE `peer_id` = ?1)
                 UNION ALL
                 SELECT 2 AS `kind`, `group_id`, `author`, `message_id`, `original_hlc` AS `sort_hlc`
                 FROM `chat_attachments` WHERE `group_id` IN
                     (SELECT `group_id` FROM `conversation_groups` WHERE `peer_id` = ?1)
             ) ORDER BY `sort_hlc`, `group_id`, `author`, `message_id` LIMIT 1",
            [peer],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((kind, group, author, id)) = selected else {
        return Err(Error::InboxConflict);
    };
    let key = MessageKey {
        group,
        author,
        id: id.try_into().map_err(|_| Error::InboxConflict)?,
    };
    tx.execute(
        "INSERT INTO `chat_cleared` (`group_id`, `author`, `message_id`, `cleared_at_ms`)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (`group_id`, `author`, `message_id`) DO UPDATE SET
         `cleared_at_ms` = excluded.`cleared_at_ms`",
        params![&key.group, &key.author, key.id.as_slice(), now],
    )?;
    remove_evicted(tx, &key, kind)
}

fn remove_evicted(tx: &Transaction<'_>, key: &MessageKey, kind: i64) -> Result<(), Error> {
    tx.execute(
        "DELETE FROM `outbox` WHERE `chat_group` = ?1 AND `chat_message_id` = ?2
         AND `accepted_at_ms` IS NULL",
        params![&key.group, key.id.as_slice()],
    )?;
    tx.execute(
        "DELETE FROM `chat_operations` WHERE `group_id` = ?1 AND `author` = ?2 AND `target_id` = ?3",
        params![&key.group, &key.author, key.id.as_slice()],
    )?;
    let table = match kind {
        1 => "chat_messages",
        2 => "chat_attachments",
        _ => return Err(Error::InboxConflict),
    };
    tx.execute(
        &format!(
            "DELETE FROM `{table}` WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3"
        ),
        params![&key.group, &key.author, key.id.as_slice()],
    )?;
    Ok(())
}

pub(in crate::store) fn purge_replay(tx: &Transaction<'_>, now: i64) -> Result<(), Error> {
    let before = now
        .checked_sub(REPLAY_RETENTION_MS)
        .ok_or(Error::ClockRange)?;
    tx.execute("DELETE FROM `inbox` WHERE `received_at_ms` <= ?1", [before])?;
    tx.execute(
        "DELETE FROM `chat_cleared` WHERE `cleared_at_ms` <= ?1",
        [before],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::Body;
    use crate::store::messages::VerifiedChatSender;
    use crate::store::{InboxKey, InboxKind, Store};
    use dyapp_identity::Identity;
    use std::path::PathBuf;

    struct Fixture {
        store: Store,
        owner: Identity,
        peer: String,
        other: String,
        path: PathBuf,
    }

    fn message_id(number: i64) -> [u8; 16] {
        u128::try_from(number).unwrap().to_be_bytes()
    }

    fn key(group: &[u8], author: &str, number: i64) -> MessageKey {
        MessageKey {
            group: group.to_vec(),
            author: author.to_owned(),
            id: message_id(number),
        }
    }

    fn fixture() -> Fixture {
        let path = crate::store::tests::temp_db_path();
        let owner = Identity::try_generate().unwrap();
        let peer = Identity::try_generate().unwrap().peer_id();
        let other = Identity::try_generate().unwrap().peer_id();
        let mut store = Store::open(&path, &owner).unwrap();
        crate::store::tests::catalog_group(&mut store, &[3], &peer);
        crate::store::tests::catalog_group(&mut store, &[4], &other);
        let tx = store.db.transaction().unwrap();
        for number in 1..=MAX_CONTACT_ITEMS {
            tx.execute(
                "INSERT INTO `chat_messages` (`group_id`, `author`, `message_id`, `text`,
                 `original_digest`, `original_hlc`, `unread`, `edited`, `deleted`, `forwarded`)
                 VALUES (x'03', ?1, ?2, x'01', zeroblob(32), ?3, 1, 0, 0, 0)",
                params![&peer, message_id(number).as_slice(), number],
            )
            .unwrap();
        }
        tx.execute(
            "INSERT INTO `chat_messages` (`group_id`, `author`, `message_id`, `text`,
             `original_digest`, `original_hlc`, `unread`, `edited`, `deleted`, `forwarded`)
             VALUES (x'04', ?1, ?2, x'01', zeroblob(32), 0, 1, 0, 0, 0)",
            params![&other, message_id(255).as_slice()],
        )
        .unwrap();
        tx.commit().unwrap();
        Fixture {
            store,
            owner,
            peer,
            other,
            path,
        }
    }

    fn inbound(number: i64) -> (InboxKey, Body) {
        (
            InboxKey {
                mailbox: [1; 32],
                id: message_id(number),
            },
            Body::Text {
                id: message_id(number),
                hlc: Some(u64::try_from(number).unwrap()),
                text: vec![1],
            },
        )
    }

    #[test]
    fn exact_contact_limit_evicts_oldest_and_survives_restart() {
        let mut f = fixture();
        let (inbox, body) = inbound(MAX_CONTACT_ITEMS + 1);
        let sender = VerifiedChatSender {
            group: &[3],
            author: &f.peer,
            device: &[5; 32],
        };
        assert!(f
            .store
            .stage_chat_inbound(b"mls", &inbox, sender, &body, 1_000_000)
            .unwrap());
        assert!(f
            .store
            .chat_message(&key(&[3], &f.peer, 1))
            .unwrap()
            .is_none());
        assert!(f
            .store
            .chat_message(&key(&[3], &f.peer, 2))
            .unwrap()
            .is_some());
        assert_eq!(
            f.store.current_chat_messages(&f.peer).unwrap().len(),
            10_000
        );
        assert_eq!(f.store.current_chat_messages(&f.other).unwrap().len(), 1);
        f.store.set_blocked(&f.peer, true).unwrap();
        f.store.set_muted(&f.peer, true).unwrap();
        drop(f.store);

        f.store = Store::open(&f.path, &f.owner).unwrap();
        assert_eq!(
            f.store.current_chat_messages(&f.peer).unwrap().len(),
            10_000
        );
        assert!(f.store.blocked(&f.peer).unwrap());
        assert!(f.store.muted(&f.peer).unwrap());
        assert!(f
            .store
            .chat_message(&key(&[4], &f.other, 255))
            .unwrap()
            .is_some());
    }

    #[test]
    fn failed_eviction_rolls_back_new_history_inbox_and_ack() {
        let mut f = fixture();
        f.store
            .db
            .execute_batch(
                "CREATE TRIGGER `fail_quota_eviction` BEFORE DELETE ON `chat_messages`
                 BEGIN SELECT RAISE(FAIL, 'injected delete failure'); END",
            )
            .unwrap();
        let (inbox, body) = inbound(MAX_CONTACT_ITEMS + 1);
        let sender = VerifiedChatSender {
            group: &[3],
            author: &f.peer,
            device: &[5; 32],
        };
        assert!(f
            .store
            .stage_chat_inbound(b"mls", &inbox, sender, &body, 1_000_000)
            .is_err());
        assert!(!f.store.has_inbox(&inbox).unwrap());
        assert!(f
            .store
            .chat_message(&key(&[3], &f.peer, 1))
            .unwrap()
            .is_some());
        assert!(f
            .store
            .chat_message(&key(&[3], &f.peer, MAX_CONTACT_ITEMS + 1))
            .unwrap()
            .is_none());
    }

    #[test]
    fn replay_metadata_expires_only_at_the_retention_boundary() {
        let owner = Identity::try_generate().unwrap();
        let mut store = Store::open(&crate::store::tests::temp_db_path(), &owner).unwrap();
        let inbox = InboxKey {
            mailbox: [1; 32],
            id: [2; 16],
        };
        store
            .stage_inbound(b"mls", &inbox, InboxKind::Message, b"digest", None)
            .unwrap();
        let base = REPLAY_RETENTION_MS + 1_000;
        store
            .db
            .execute(
                "UPDATE `inbox` SET `received_at_ms` = ?1",
                [base - REPLAY_RETENTION_MS],
            )
            .unwrap();
        store
            .db
            .execute(
                "INSERT INTO `chat_cleared` (`group_id`, `author`, `message_id`, `cleared_at_ms`)
                 VALUES (x'03', 'peer', ?2, ?1)",
                params![base - REPLAY_RETENTION_MS, [1u8; 16].as_slice()],
            )
            .unwrap();
        let tx = store.db.transaction().unwrap();
        purge_replay(&tx, base - 1).unwrap();
        tx.commit().unwrap();
        assert!(store.has_inbox(&inbox).unwrap());
        let tx = store.db.transaction().unwrap();
        purge_replay(&tx, base).unwrap();
        tx.commit().unwrap();
        assert!(!store.has_inbox(&inbox).unwrap());
        let cleared: i64 = store
            .db
            .query_row("SELECT COUNT(*) FROM `chat_cleared`", [], |row| row.get(0))
            .unwrap();
        assert_eq!(cleared, 0);
    }
}
