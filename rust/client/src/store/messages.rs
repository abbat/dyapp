//! Current chat text projection. Inbox envelope IDs remain a separate transport dedupe key.

use super::conversations;
use super::likes::{load_hlc, save_hlc};
use super::{Error, InboxKey, InboxKind, Store};
use crate::chat::{Body, MAX_TEXT_BYTES};
use crate::PreparedEnvelope;
use dyapp_identity::sha256;
use dyapp_messaging::hlc::{ClockSkew, HybridClock, Timestamp, MAX_CLOCK_SKEW_MS};
use dyapp_messaging::mls::Group;
use rusqlite::{params, OptionalExtension, Transaction};

pub(crate) mod attachments;
pub use attachments::{ChatAttachment, ChatItem};
mod clear;
pub(super) use clear::clear_pair_tx;
#[cfg(test)]
mod inbound_tests;
mod media;
mod operations;
mod sticker_operations;
mod stickers;
pub use stickers::ChatSticker;
#[cfg(test)]
mod outbound_tests;
#[cfg(test)]
mod read_tests;

pub(super) const SCHEMA: &str = r"
CREATE TABLE `chat_messages` (
    `arrival_order` INTEGER PRIMARY KEY AUTOINCREMENT,
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `author` TEXT NOT NULL CHECK (length(`author`) > 0),
    `message_id` BLOB NOT NULL CHECK (length(`message_id`) = 16),
    `text` BLOB,
    `original_digest` BLOB CHECK (`original_digest` IS NULL OR length(`original_digest`) = 32),
    `edited` INTEGER NOT NULL CHECK (`edited` IN (0, 1)),
    `deleted` INTEGER NOT NULL CHECK (`deleted` IN (0, 1)),
    `forwarded` INTEGER NOT NULL DEFAULT 0 CHECK (`forwarded` IN (0, 1)),
    `last_hlc` INTEGER,
    `last_device` BLOB,
    `last_operation` BLOB CHECK (`last_operation` IS NULL OR length(`last_operation`) = 16),
    `original_hlc` INTEGER CHECK (`original_hlc` IS NULL OR `original_hlc` >= 0),
    `unread` INTEGER NOT NULL DEFAULT 0 CHECK (`unread` IN (0, 1)),
    -- Own current content: pending=0, node-accepted=1, device-received=2.
    `delivery_generation` BLOB CHECK (`delivery_generation` IS NULL OR length(`delivery_generation`) = 16),
    `delivery_state` INTEGER CHECK (`delivery_state` IS NULL OR `delivery_state` IN (0, 1, 2)),
    CHECK ((`delivery_generation` IS NULL) = (`delivery_state` IS NULL)),
    CHECK ((`last_hlc` IS NULL AND `last_device` IS NULL AND `last_operation` IS NULL)
        OR (`last_hlc` IS NOT NULL AND `last_device` IS NOT NULL AND `last_operation` IS NOT NULL)),
    CHECK ((`deleted` = 0 AND `text` IS NOT NULL AND `original_digest` IS NOT NULL)
        OR (`deleted` = 1 AND `text` IS NULL)),
    UNIQUE (`group_id`, `author`, `message_id`)
) STRICT;
CREATE TABLE `chat_attachments` (
    `media_kind` INTEGER NOT NULL DEFAULT 1 CHECK (`media_kind` IN (1, 2)),
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `author` TEXT NOT NULL CHECK (length(`author`) > 0),
    `message_id` BLOB NOT NULL CHECK (length(`message_id`) = 16),
    `original_hlc` INTEGER NOT NULL CHECK (`original_hlc` >= 0),
    `descriptor` BLOB NOT NULL CHECK (length(`descriptor`) > 0),
    `forwarded` INTEGER NOT NULL CHECK (`forwarded` IN (0, 1)),
    `unread` INTEGER NOT NULL CHECK (`unread` IN (0, 1)),
    `delivery_generation` BLOB CHECK (`delivery_generation` IS NULL OR length(`delivery_generation`) = 16),
    `delivery_state` INTEGER CHECK (`delivery_state` IS NULL OR `delivery_state` IN (0, 1, 2)),
    CHECK ((`delivery_generation` IS NULL) = (`delivery_state` IS NULL)),
    PRIMARY KEY (`group_id`, `author`, `message_id`)
) STRICT;
CREATE TABLE `chat_operations` (
    `group_id` BLOB NOT NULL,
    `author` TEXT NOT NULL,
    `operation_id` BLOB NOT NULL CHECK (length(`operation_id`) = 16),
    `target_id` BLOB NOT NULL CHECK (length(`target_id`) = 16),
    `device` BLOB NOT NULL CHECK (length(`device`) = 32),
    `digest` BLOB NOT NULL CHECK (length(`digest`) = 32),
    `hlc` INTEGER NOT NULL,
    `pending` INTEGER NOT NULL CHECK (`pending` IN (0, 1)),
    `pending_text` BLOB CHECK (`pending_text` IS NULL OR length(`pending_text`) <= 65536),
    CHECK (`pending` = 1 OR `pending_text` IS NULL),
    PRIMARY KEY (`group_id`, `author`, `operation_id`)
) STRICT;
CREATE TABLE `chat_cleared` (
    `group_id` BLOB NOT NULL,
    `author` TEXT NOT NULL,
    `message_id` BLOB NOT NULL CHECK (length(`message_id`) = 16),
    PRIMARY KEY (`group_id`, `author`, `message_id`)
) STRICT;
";

/// One logical message, shared by recipient devices and independent of envelope IDs.
#[derive(Clone, PartialEq, Eq)]
pub struct MessageKey {
    /// MLS group ID.
    pub group: Vec<u8>,
    /// Verified MLS author Peer ID.
    pub author: String,
    /// Random ID carried by chat text tag 1 or 12.
    pub id: [u8; 16],
}

/// Sender metadata that the caller verified through MLS before staging a chat body.
#[derive(Clone, Copy)]
pub struct VerifiedChatSender<'a> {
    /// Group ID from the processed MLS application message.
    pub group: &'a [u8],
    /// Verified sender Peer ID.
    pub author: &'a str,
    /// Verified sender device key used for operation ordering.
    pub device: &'a [u8],
}

/// The sole current content of one message; prior revisions are not app history.
#[derive(Clone, PartialEq, Eq)]
pub struct CurrentMessage {
    /// Present text, or `None` for a deletion placeholder.
    pub text: Option<Vec<u8>>,
    /// Whether a later edit replaced the original text.
    pub edited: bool,
    /// Whether deletion is terminal for this message.
    pub deleted: bool,
}

/// Persisted status of the current own text or edit/delete operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryState {
    /// Awaiting acceptance by the recipient's mailbox node.
    Pending,
    /// The mailbox node accepted the envelope.
    NodeAccepted,
    /// The recipient device acknowledged the envelope.
    DeviceReceived,
}

impl DeliveryState {
    pub(super) fn from_stored(value: Option<i64>) -> Result<Option<Self>, Error> {
        match value {
            None => Ok(None),
            Some(0) => Ok(Some(Self::Pending)),
            Some(1) => Ok(Some(Self::NodeAccepted)),
            Some(2) => Ok(Some(Self::DeviceReceived)),
            _ => Err(Error::InboxConflict),
        }
    }
}

/// One current message in a pair's ordered local history.
#[derive(Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// Logical key, including the MLS group that carried this message.
    pub key: MessageKey,
    /// The sole current text or deletion placeholder.
    pub current: CurrentMessage,
    /// Authenticated original time; absent for legacy tag-1 bodies.
    pub original_hlc: Option<Timestamp>,
    /// Whether this device has not yet viewed the incoming text.
    pub unread: bool,
    /// The MLS sender forwarded this text; no original author is asserted.
    pub forwarded: bool,
    /// Absent for peer text or an own text received from another device.
    pub delivery: Option<DeliveryState>,
}

impl CurrentMessage {
    pub(super) fn from_stored(
        text: Option<Vec<u8>>,
        edited: i64,
        deleted: i64,
    ) -> Result<Self, Error> {
        if !matches!(
            (edited, deleted, text.is_some()),
            (0 | 1, 0, true) | (0 | 1, 1, false)
        ) {
            return Err(Error::InboxConflict);
        }
        Ok(Self {
            text,
            edited: edited == 1,
            deleted: deleted == 1,
        })
    }
}

impl Store {
    /// Draft the original time for one text without changing durable state. The staged MLS
    /// body and the outbox commit must use this exact value.
    pub fn draft_chat_text(&self, wall_ms: u64) -> Result<u64, Error> {
        Ok(HybridClock::resume(load_hlc(&self.db)?)
            .now_at(wall_ms)
            .into())
    }

    /// Draft a fresh operation ID and HLC without changing durable state. Encrypt this exact
    /// operation before committing it with `stage_chat_outbound`.
    pub fn draft_chat_operation(&self, wall_ms: u64) -> Result<([u8; 16], u64), Error> {
        let hlc = self.draft_chat_text(wall_ms)?;
        let mut operation = [0; 16];
        getrandom::fill(&mut operation).map_err(|_| Error::Random)?;
        Ok((operation, hlc))
    }

    /// Stage an own chat body, current projection, sealed MLS state and exact writes together.
    /// The caller supplies its certified device key; the author must be this store's owner.
    pub fn stage_chat_outbound(
        &mut self,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
        sender: VerifiedChatSender<'_>,
        body: &Body,
        wall_ms: u64,
    ) -> Result<(), Error> {
        let tx = self.db.transaction()?;
        Self::stage_chat_outbound_tx(
            &tx,
            &self.owner_peer_id,
            sealed_device,
            writes,
            sender,
            body,
            wall_ms,
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Catalog a new pair group and commit its Welcome writes with the first text in it, so a
    /// retry after a crash resends both with the same bytes.
    #[expect(
        clippy::too_many_arguments,
        reason = "the group, its Welcome and the text commit together"
    )]
    pub fn stage_invite_text(
        &mut self,
        group: &Group,
        peer_id: &str,
        sealed_device: &[u8],
        welcome: &[PreparedEnvelope],
        writes: &[PreparedEnvelope],
        body: &Body,
        device: &[u8],
        wall_ms: u64,
    ) -> Result<(), Error> {
        if peer_id == self.owner_peer_id || !group.members()?.contains(&peer_id.to_owned()) {
            return Err(Error::GroupConflict);
        }
        let sender = VerifiedChatSender {
            group: group.id(),
            author: &self.owner_peer_id,
            device,
        };
        let tx = self.db.transaction()?;
        Self::stage_outbound_tx(
            &tx,
            Some((group.id(), peer_id)),
            sealed_device,
            welcome,
            true,
        )?;
        Self::stage_chat_outbound_tx(
            &tx,
            &self.owner_peer_id,
            sealed_device,
            writes,
            sender,
            body,
            wall_ms,
        )?;
        tx.commit()?;
        Ok(())
    }

    fn stage_chat_outbound_tx(
        tx: &Transaction<'_>,
        owner: &str,
        sealed_device: &[u8],
        writes: &[PreparedEnvelope],
        sender: VerifiedChatSender<'_>,
        body: &Body,
        wall_ms: u64,
    ) -> Result<(), Error> {
        if sender.author != owner || sender.group.is_empty() || sender.device.len() != 32 {
            return Err(Error::InboxConflict);
        }
        let (id, generation) = match body {
            Body::Text { id, .. } => (id, id),
            Body::Attachment(message) => (&message.id, &message.id),
            Body::Forwarded(inner) => match inner.as_ref() {
                Body::Text { id, .. } => (id, id),
                Body::Attachment(message) => (&message.id, &message.id),
                _ => return Err(Error::InboxConflict),
            },
            Body::Edit {
                target, operation, ..
            }
            | Body::Delete {
                target, operation, ..
            } => (target, operation),
            _ => return Err(Error::InboxConflict),
        };
        let key = MessageKey {
            group: sender.group.to_vec(),
            author: sender.author.to_owned(),
            id: *id,
        };
        if let Body::Edit { hlc, .. } | Body::Delete { hlc, .. } = body {
            if Timestamp::from(*hlc) <= load_hlc(tx)? {
                return Err(Error::InboxConflict);
            }
        }
        let timed_text = match body {
            Body::Text { hlc: Some(hlc), .. } => Some(Timestamp::from(*hlc)),
            Body::Forwarded(inner) => match inner.as_ref() {
                Body::Text { hlc: Some(hlc), .. } => Some(Timestamp::from(*hlc)),
                _ => None,
            },
            _ => None,
        }
        .or_else(|| {
            body.attachment_message()
                .map(|message| Timestamp::from(message.hlc))
        });
        if let Some(hlc) = timed_text {
            if hlc <= load_hlc(tx)? {
                return Err(Error::InboxConflict);
            }
            if hlc.wall_ms() > wall_ms.saturating_add(MAX_CLOCK_SKEW_MS) {
                return Err(Error::ClockSkew(ClockSkew {
                    remote_ms: hlc.wall_ms(),
                    local_ms: wall_ms,
                }));
            }
            i64::try_from(u64::from(hlc)).map_err(|_| Error::ClockRange)?;
        }
        if body.attachment_message().is_some() {
            if !conversations::on_attachment(tx, owner, sender.group, sender.author, *id)? {
                return Err(Error::RequestPending);
            }
        } else if matches!(body, Body::Text { .. } | Body::Forwarded(_)) {
            conversations::on_text(tx, owner, sender.group, sender.author, *id, true)?;
        } else if !conversations::allows_operation(tx, owner, sender.group, sender.author, *id)? {
            return Err(Error::ChatClosed);
        }
        Self::stage_outbound_tx(tx, None, sealed_device, writes, true)?;
        for write in writes {
            let linked = tx.execute(
                "UPDATE `outbox` SET `chat_group` = ?3, `chat_message_id` = ?4,
                 `chat_generation_id` = ?5 WHERE `mailbox` = ?1 AND `envelope_id` = ?2
                 AND (`chat_group` IS NULL OR `chat_group` = ?3)
                 AND (`chat_message_id` IS NULL OR `chat_message_id` = ?4)
                 AND (`chat_generation_id` IS NULL OR `chat_generation_id` = ?5)",
                params![
                    write.mailbox().as_slice(),
                    write.id().as_slice(),
                    sender.group,
                    id.as_slice(),
                    generation.as_slice()
                ],
            )?;
            if linked != 1 {
                return Err(Error::OutboxConflict);
            }
        }
        let changed = if body.attachment_message().is_some() {
            Self::record_chat_attachment_tx(tx, owner, sender, body, false)?
        } else {
            match body {
                Body::Text { hlc, text, .. } => {
                    Self::record_chat_text_tx(tx, &key, text, *hlc, false, false)?
                }
                Body::Forwarded(inner) => match inner.as_ref() {
                    Body::Text { hlc, text, .. } => {
                        Self::record_chat_text_tx(tx, &key, text, *hlc, false, true)?
                    }
                    _ => unreachable!(),
                },
                Body::Edit { .. } | Body::Delete { .. } => {
                    Self::apply_chat_operation_tx(tx, &key, sender.device, body, wall_ms)?
                }
                _ => unreachable!(),
            }
        };
        // An outbound action must change local state; replay uses the exact existing outbox write.
        if !changed {
            return Err(Error::InboxConflict);
        }
        let updated = if body.attachment_message().is_some() {
            tx.execute(
                "UPDATE `chat_attachments` SET `delivery_generation` = ?4, `delivery_state` = 0
                 WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![sender.group, owner, id.as_slice(), generation.as_slice()],
            )?
        } else {
            tx.execute(
                "UPDATE `chat_messages` SET `delivery_generation` = ?4, `delivery_state` = 0
                 WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![sender.group, owner, id.as_slice(), generation.as_slice()],
            )?
        };
        if updated != 1 {
            return Err(Error::InboxConflict);
        }
        if let Some(hlc) = timed_text {
            save_hlc(tx, hlc)?;
        }
        Ok(())
    }

    /// Commit one authenticated chat body, sealed MLS state, inbox dedupe and ACK intent together.
    /// The inbox retains only a body digest, never a second copy of chat content. Returns false for
    /// a duplicate envelope and for a body the request rules drop, which is still ACKed.
    pub fn stage_chat_inbound(
        &mut self,
        sealed_device: &[u8],
        inbox: &InboxKey,
        sender: VerifiedChatSender<'_>,
        body: &Body,
        wall_ms: u64,
    ) -> Result<bool, Error> {
        let id = match body {
            Body::Text { id, .. } => id,
            Body::Attachment(message) => &message.id,
            Body::Sticker(sticker) => &sticker.id,
            Body::Forwarded(inner) => match inner.as_ref() {
                Body::Text { id, .. } => id,
                Body::Attachment(message) => &message.id,
                _ => return Err(Error::InboxConflict),
            },
            Body::Edit { target, .. } | Body::Delete { target, .. } => target,
            _ => return Err(Error::InboxConflict),
        };
        if sender.group.is_empty() || sender.author.is_empty() || sender.device.len() != 32 {
            return Err(Error::InboxConflict);
        }
        let key = MessageKey {
            group: sender.group.to_vec(),
            author: sender.author.to_owned(),
            id: *id,
        };
        let digest = sha256(&body.encode());
        let owner = self.owner_peer_id.clone();
        let unread = key.author != owner;
        let mut shown = true;
        let inserted = self.stage_inbound_with(
            sealed_device,
            inbox,
            InboxKind::Message,
            &digest,
            None,
            |tx| {
                // A cleared message stays gone; its envelope is still recorded and ACKed.
                if clear::is_cleared(tx, &key)? {
                    shown = false;
                    return Ok(());
                }
                shown = if body.attachment_message().is_some() || matches!(body, Body::Sticker(_)) {
                    conversations::on_attachment(tx, &owner, &key.group, &key.author, key.id)?
                } else {
                    match body {
                        Body::Text { .. } | Body::Forwarded(_) => conversations::on_text(
                            tx,
                            &owner,
                            &key.group,
                            &key.author,
                            key.id,
                            false,
                        )?,
                        _ => conversations::allows_operation(
                            tx,
                            &owner,
                            &key.group,
                            &key.author,
                            key.id,
                        )?,
                    }
                };
                if !shown {
                    return Ok(());
                }
                if let Some(message) = body.attachment_message() {
                    i64::try_from(message.hlc).map_err(|_| Error::ClockRange)?;
                    let observed = HybridClock::resume(load_hlc(tx)?)
                        .observe_at(Timestamp::from(message.hlc), wall_ms)?;
                    save_hlc(tx, observed)?;
                    shown = Self::record_chat_attachment_tx(tx, &owner, sender, body, unread)?;
                    return Ok(());
                }
                match body {
                    Body::Sticker(sticker) => {
                        i64::try_from(sticker.hlc).map_err(|_| Error::ClockRange)?;
                        let observed = HybridClock::resume(load_hlc(tx)?)
                            .observe_at(Timestamp::from(sticker.hlc), wall_ms)?;
                        save_hlc(tx, observed)?;
                        shown = Self::record_chat_sticker_tx(tx, &owner, sender, sticker, unread)?;
                    }
                    Body::Text { hlc, text, .. } => {
                        if let Some(hlc) = hlc {
                            i64::try_from(*hlc).map_err(|_| Error::ClockRange)?;
                            let observed = HybridClock::resume(load_hlc(tx)?)
                                .observe_at(Timestamp::from(*hlc), wall_ms)?;
                            save_hlc(tx, observed)?;
                        }
                        Self::record_chat_text_tx(tx, &key, text, *hlc, unread, false)?;
                    }
                    Body::Forwarded(inner) => {
                        let Body::Text { hlc, text, .. } = inner.as_ref() else {
                            return Err(Error::InboxConflict);
                        };
                        if let Some(hlc) = hlc {
                            i64::try_from(*hlc).map_err(|_| Error::ClockRange)?;
                            let observed = HybridClock::resume(load_hlc(tx)?)
                                .observe_at(Timestamp::from(*hlc), wall_ms)?;
                            save_hlc(tx, observed)?;
                        }
                        Self::record_chat_text_tx(tx, &key, text, *hlc, unread, true)?;
                    }
                    Body::Edit { .. } | Body::Delete { .. } => {
                        Self::apply_chat_operation_tx(tx, &key, sender.device, body, wall_ms)?;
                    }
                    _ => unreachable!(),
                }
                Ok(())
            },
        )?;
        Ok(inserted && shown)
    }

    /// Mark all current incoming texts and attachments from one peer read across its MLS groups.
    /// This is device-local and does not send a peer-visible read receipt.
    pub fn mark_chat_read(&mut self, peer_id: &str) -> Result<usize, Error> {
        if peer_id.is_empty() || peer_id == self.owner_peer_id {
            return Err(Error::InboxConflict);
        }
        let tx = self.db.transaction()?;
        let texts = tx.execute(
            "UPDATE `chat_messages` SET `unread` = 0
             WHERE `unread` = 1 AND `author` = ?1
               AND `group_id` IN (SELECT `group_id` FROM `conversation_groups` WHERE `peer_id` = ?1)",
            [peer_id],
        )?;
        let attachments = tx.execute(
            "UPDATE `chat_attachments` SET `unread` = 0
             WHERE `unread` = 1 AND `author` = ?1
               AND `group_id` IN (SELECT `group_id` FROM `conversation_groups` WHERE `peer_id` = ?1)",
            [peer_id],
        )?;
        tx.commit()?;
        Ok(texts + attachments)
    }

    /// Read a pair's current texts and deletion placeholders across all cataloged MLS groups.
    /// Legacy bodies have no sent time and appear first in local arrival order; timed bodies
    /// follow original HLC, author, logical ID and group ID order.
    pub fn current_chat_messages(&self, peer_id: &str) -> Result<Vec<ChatMessage>, Error> {
        if peer_id.is_empty() || peer_id == self.owner_peer_id {
            return Err(Error::InboxConflict);
        }
        let mut query = self.db.prepare(
            "SELECT `m`.`group_id`, `m`.`author`, `m`.`message_id`, `m`.`text`,
                    `m`.`edited`, `m`.`deleted`, `m`.`original_hlc`, `m`.`unread`,
                    `m`.`delivery_state`, `m`.`forwarded`
             FROM `chat_messages` AS `m`
             JOIN `conversation_groups` AS `g` ON `m`.`group_id` = `g`.`group_id`
             WHERE `g`.`peer_id` = ?1
             ORDER BY (`m`.`original_hlc` IS NOT NULL),
                      CASE WHEN `m`.`original_hlc` IS NULL THEN `m`.`arrival_order` END,
                      `m`.`original_hlc`, `m`.`author`, `m`.`message_id`, `m`.`group_id`",
        )?;
        let rows = query.query_map([peer_id], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, i64>(9)?,
            ))
        })?;
        rows.map(|row| {
            let (
                group,
                author,
                id,
                text,
                edited,
                deleted,
                original_hlc,
                unread,
                delivery,
                forwarded,
            ) = row?;
            if author != self.owner_peer_id && author != peer_id {
                return Err(Error::InboxConflict);
            }
            let delivery = DeliveryState::from_stored(delivery)?;
            if author != self.owner_peer_id && delivery.is_some() {
                return Err(Error::InboxConflict);
            }
            let id = id.try_into().map_err(|_| Error::InboxConflict)?;
            let current = CurrentMessage::from_stored(text, edited, deleted)?;
            let original_hlc = original_hlc
                .map(|value| {
                    u64::try_from(value)
                        .map(Timestamp::from)
                        .map_err(|_| Error::InboxConflict)
                })
                .transpose()?;
            let unread = match unread {
                0 => false,
                1 => true,
                _ => return Err(Error::InboxConflict),
            };
            Ok(ChatMessage {
                key: MessageKey { group, author, id },
                current,
                original_hlc,
                unread,
                forwarded: forwarded == 1,
                delivery,
            })
        })
        .collect()
    }

    /// Read one logical message from this profile's current projection.
    pub fn chat_message(&self, key: &MessageKey) -> Result<Option<CurrentMessage>, Error> {
        let row: Option<(Option<Vec<u8>>, i64, i64)> = self
            .db
            .query_row(
                "SELECT `text`, `edited`, `deleted` FROM `chat_messages`
             WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![&key.group, &key.author, key.id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        row.map(|(text, edited, deleted)| CurrentMessage::from_stored(text, edited, deleted))
            .transpose()
    }

    /// Store first text once. A repeated body is harmless; a different body with the same
    /// logical key is a conflict. The caller has already verified the author through MLS.
    pub fn record_chat_text(&mut self, key: &MessageKey, text: &[u8]) -> Result<bool, Error> {
        let tx = self.db.transaction()?;
        let changed = Self::record_chat_text_tx(&tx, key, text, None, false, false)?;
        tx.commit()?;
        Ok(changed)
    }

    pub(super) fn record_chat_text_tx(
        tx: &Transaction<'_>,
        key: &MessageKey,
        text: &[u8],
        hlc: Option<u64>,
        unread: bool,
        forwarded: bool,
    ) -> Result<bool, Error> {
        if key.group.is_empty() || key.author.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err(Error::InboxConflict);
        }
        if attachments::exists(tx, key)? {
            return Err(Error::InboxConflict);
        }
        let original_hlc = hlc
            .map(i64::try_from)
            .transpose()
            .map_err(|_| Error::ClockRange)?;
        let digest = sha256(text);
        let saved: Option<(Option<Vec<u8>>, Option<i64>, i64)> = tx
            .query_row(
                "SELECT `original_digest`, `original_hlc`, `forwarded` FROM `chat_messages`
             WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                params![&key.group, &key.author, key.id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        match saved {
            Some((Some(original_digest), saved_hlc, saved_forwarded)) => {
                if original_digest != digest
                    || saved_hlc != original_hlc
                    || saved_forwarded != i64::from(forwarded)
                {
                    return Err(Error::InboxConflict);
                }
                // An original delivered after edit/delete must not replace the projection.
                Ok(false)
            }
            Some((None, Some(_), _)) => Err(Error::InboxConflict),
            Some((None, None, _)) => {
                // A delete reached us before its original; record its digest and time.
                tx.execute(
                    "UPDATE `chat_messages` SET `original_digest` = ?4, `original_hlc` = ?5,
                         `forwarded` = ?6 WHERE `group_id` = ?1 AND `author` = ?2 AND `message_id` = ?3",
                    params![
                        &key.group,
                        &key.author,
                        key.id.as_slice(),
                        digest.as_slice(),
                        original_hlc,
                        i64::from(forwarded)
                    ],
                )?;
                operations::resolve_pending(tx, key, true)?;
                Ok(false)
            }
            None => {
                tx.execute(
                    "INSERT INTO `chat_messages` (`group_id`, `author`, `message_id`, `text`, `original_digest`, `original_hlc`, `unread`, `edited`, `deleted`, `forwarded`)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, ?8)",
                    params![&key.group, &key.author, key.id.as_slice(), text, digest.as_slice(), original_hlc, i64::from(unread), i64::from(forwarded)],
                )?;
                operations::resolve_pending(tx, key, false)?;
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MessageKey, Store};
    use crate::store::Error;
    use dyapp_identity::Identity;

    #[test]
    fn text_projection_survives_restart_and_refuses_conflicts() {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "dyapp-chat-{:032x}.db",
            u128::from_be_bytes(random)
        ));
        let alice = Identity::generate();
        let bob = Identity::generate();
        let key = MessageKey {
            group: vec![3],
            author: alice.peer_id(),
            id: [4; 16],
        };
        let mut store = Store::open(&path, &alice).unwrap();
        assert!(store.chat_message(&key).unwrap().is_none());
        assert!(store.record_chat_text(&key, b"hello").unwrap());
        assert!(!store.record_chat_text(&key, b"hello").unwrap());
        assert!(matches!(
            store.record_chat_text(&key, b"other"),
            Err(Error::InboxConflict)
        ));
        drop(store);
        assert!(matches!(
            Store::open(&path, &bob),
            Err(Error::WrongIdentity)
        ));
        let store = Store::open(&path, &alice).unwrap();
        let message = store.chat_message(&key).unwrap().unwrap();
        assert_eq!(message.text.as_deref(), Some(&b"hello"[..]));
        assert!(!message.edited && !message.deleted);
    }

    #[test]
    fn legacy_arrival_order_is_not_reused_after_clear_and_restart() {
        let path = crate::store::tests::temp_db_path();
        let owner = Identity::generate();
        let key = |id| MessageKey {
            group: vec![3],
            author: owner.peer_id(),
            id: [id; 16],
        };
        let mut store = Store::open(&path, &owner).unwrap();
        assert!(store.record_chat_text(&key(1), b"first").unwrap());
        assert!(store.record_chat_text(&key(2), b"second").unwrap());
        let before: (i64, i64, Option<i64>) = store.db.query_row(
            "SELECT `rowid`, `arrival_order`, `original_hlc` FROM `chat_messages` WHERE `message_id` = ?1",
            [&[2u8; 16][..]],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        assert_eq!(before.0, before.1);
        assert_eq!(before.2, None);
        assert_eq!(store.clear_chat(&[3]).unwrap(), 2);
        drop(store);

        let mut store = Store::open(&path, &owner).unwrap();
        assert!(store.record_chat_text(&key(3), b"third").unwrap());
        let after: (i64, Option<i64>) = store.db.query_row(
            "SELECT `arrival_order`, `original_hlc` FROM `chat_messages` WHERE `message_id` = ?1",
            [&[3u8; 16][..]],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert!(after.0 > before.1);
        assert_eq!(after.1, None);
        assert!(!store.record_chat_text(&key(3), b"third").unwrap());
    }
}
