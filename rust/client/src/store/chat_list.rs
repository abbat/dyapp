//! Pair-wide Chats list projection from local conversations and current message rows.

use super::messages::attachments::ItemOrder;
use super::messages::{
    ChatAttachment, ChatMessage, ChatSticker, CurrentMessage, DeliveryState, MessageKey,
};
use super::{Conversation, Error, Store};
use dyapp_messaging::hlc::Timestamp;

/// One Chats row for this profile. A cleared conversation remains with no last message.
#[derive(Clone, PartialEq, Eq)]
pub struct ChatRow {
    /// The other identity.
    pub peer_id: String,
    /// Request or open state, independent of local history deletion.
    pub state: Conversation,
    /// Latest current text or deletion placeholder; absent after Clear chat.
    pub last_message: Option<ChatMessage>,
    /// The newest attachment when it is newer than `last_message`: the row's preview.
    pub last_attachment: Option<ChatAttachment>,
    /// Newest sticker when it is newer than both the text and photo preview.
    pub last_sticker: Option<ChatSticker>,
    /// Incoming current texts and media not yet viewed on this device.
    pub unread_count: usize,
}

impl ChatRow {
    fn latest(&self) -> Option<ItemOrder<'_>> {
        let text = || self.last_message.as_ref().and_then(ChatMessage::order);
        self.last_sticker
            .as_ref()
            .map(ChatSticker::order)
            .or_else(|| self.last_attachment.as_ref().map(ChatAttachment::order))
            .or_else(text)
    }

    /// Timed rows, then legacy rows, then empty conversations.
    fn group(&self) -> (bool, bool) {
        let empty = self.last_message.is_none()
            && self.last_attachment.is_none()
            && self.last_sticker.is_none();
        (empty, self.latest().is_none())
    }
}

impl Store {
    /// Read active conversations, latest texts/media and local unread counts.
    /// A closed conversation with nothing left in it is not listed.
    /// Timed rows sort first by original HLC; legacy rows use local arrival order. Empty
    /// conversations follow rows with messages, then Peer ID breaks any remaining tie.
    ///
    /// # Errors
    ///
    /// Returns an error if querying or decoding conversation, message, or media
    /// state fails, or stored metadata, timestamps, or unread counts are invalid.
    pub fn chat_rows(&self) -> Result<Vec<ChatRow>, Error> {
        let mut query = self.db.prepare(
            "WITH `ranked` AS (
                SELECT `g`.`peer_id`, `m`.`group_id`, `m`.`author`, `m`.`message_id`,
                       `m`.`text`, `m`.`edited`, `m`.`deleted`, `m`.`original_hlc`,
                       `m`.`unread`, `m`.`delivery_state`, `m`.`forwarded`, `m`.`arrival_order`,
                       ROW_NUMBER() OVER (
                           PARTITION BY `g`.`peer_id`
                           ORDER BY (`m`.`original_hlc` IS NOT NULL) DESC,
                                    CASE WHEN `m`.`original_hlc` IS NULL
                                         THEN `m`.`arrival_order` END DESC,
                                    `m`.`original_hlc` DESC, `m`.`author` DESC,
                                    `m`.`message_id` DESC, `m`.`group_id` DESC
                       ) AS `message_rank`,
                       SUM(`m`.`unread`) OVER (PARTITION BY `g`.`peer_id`) AS `unread_count`
                FROM `chat_messages` AS `m`
                JOIN `conversation_groups` AS `g` ON `m`.`group_id` = `g`.`group_id`
            )
            SELECT `c`.`peer_id`, `c`.`state`, `c`.`request_id`,
                   `r`.`group_id`, `r`.`author`, `r`.`message_id`, `r`.`text`,
                   `r`.`edited`, `r`.`deleted`, `r`.`original_hlc`, `r`.`unread`,
                   `r`.`delivery_state`, `r`.`forwarded`, COALESCE(`r`.`unread_count`, 0)
            FROM `conversations` AS `c`
            LEFT JOIN `ranked` AS `r`
              ON `c`.`peer_id` = `r`.`peer_id` AND `r`.`message_rank` = 1
            WHERE `c`.`state` <> 4 OR `r`.`group_id` IS NOT NULL OR EXISTS (
                SELECT 1 FROM `chat_attachments` AS `a`
                JOIN `conversation_groups` AS `g` ON `a`.`group_id` = `g`.`group_id`
                WHERE `g`.`peer_id` = `c`.`peer_id`
            )
            ORDER BY (`r`.`group_id` IS NULL),
                     (`r`.`original_hlc` IS NOT NULL) DESC,
                     CASE WHEN `r`.`original_hlc` IS NULL
                          THEN `r`.`arrival_order` END DESC,
                     `r`.`original_hlc` DESC, `r`.`author` DESC,
                     `r`.`message_id` DESC, `r`.`group_id` DESC, `c`.`peer_id`",
        )?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<Vec<u8>>>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<Vec<u8>>>(5)?,
                row.get::<_, Option<Vec<u8>>>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, Option<i64>>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, Option<i64>>(11)?,
                row.get::<_, Option<i64>>(12)?,
                row.get::<_, i64>(13)?,
            ))
        })?;
        let mut attachments = self.latest_chat_attachments()?;
        let mut stickers = self.latest_chat_stickers()?;
        let mut result = Vec::new();
        for row in rows {
            let (
                peer_id,
                state,
                request,
                group,
                author,
                id,
                text,
                edited,
                deleted,
                time,
                unread,
                delivery,
                forwarded,
                count,
            ) = row?;
            let request_id = || -> Result<[u8; 16], Error> {
                request
                    .as_ref()
                    .and_then(|id| id.as_slice().try_into().ok())
                    .ok_or(Error::InboxConflict)
            };
            let state = match state {
                1 => Conversation::RequestSent(request_id()?),
                2 => Conversation::RequestReceived(request_id()?),
                3 if request.is_none() => Conversation::Open,
                4 if request.is_none() => Conversation::Closed,
                _ => return Err(Error::InboxConflict),
            };
            let last_message = if let Some(group) = group {
                let author = author.ok_or(Error::InboxConflict)?;
                if author != self.owner_peer_id && author != peer_id {
                    return Err(Error::InboxConflict);
                }
                let delivery = DeliveryState::from_stored(delivery)?;
                if author != self.owner_peer_id && delivery.is_some() {
                    return Err(Error::InboxConflict);
                }
                let id = id
                    .ok_or(Error::InboxConflict)?
                    .try_into()
                    .map_err(|_| Error::InboxConflict)?;
                let current = CurrentMessage::from_stored(
                    text,
                    edited.ok_or(Error::InboxConflict)?,
                    deleted.ok_or(Error::InboxConflict)?,
                )?;
                let original_hlc = time
                    .map(|value| {
                        u64::try_from(value)
                            .map(Timestamp::from)
                            .map_err(|_| Error::InboxConflict)
                    })
                    .transpose()?;
                let unread = match unread {
                    Some(0) => false,
                    Some(1) => true,
                    _ => return Err(Error::InboxConflict),
                };
                Some(ChatMessage {
                    key: MessageKey { group, author, id },
                    current,
                    original_hlc,
                    unread,
                    forwarded: forwarded == Some(1),
                    delivery,
                })
            } else {
                None
            };
            let (last_attachment, unread_attachments) = match attachments.remove(&peer_id) {
                Some((attachment, count)) => (Some(attachment), count),
                None => (None, 0),
            };
            let mut last_attachment = last_attachment.filter(|attachment| {
                let text = last_message.as_ref().and_then(ChatMessage::order);
                text < Some(attachment.order())
            });
            let (last_sticker, unread_stickers) = match stickers.remove(&peer_id) {
                Some((sticker, count)) => (Some(sticker), count),
                None => (None, 0),
            };
            let last_sticker = last_sticker.filter(|sticker| {
                last_message.as_ref().and_then(ChatMessage::order) < Some(sticker.order())
                    && last_attachment.as_ref().map(ChatAttachment::order) < Some(sticker.order())
            });
            if last_sticker.is_some() {
                last_attachment = None;
            }
            let unread_texts = usize::try_from(count).map_err(|_| Error::InboxConflict)?;
            result.push(ChatRow {
                peer_id,
                state,
                last_message,
                last_attachment,
                last_sticker,
                unread_count: unread_texts + unread_attachments + unread_stickers,
            });
        }
        // The sort is stable, so legacy and empty rows keep the order the query gave them.
        result.sort_by(|a, b| {
            let newest_first = || b.latest().cmp(&a.latest());
            a.group().cmp(&b.group()).then_with(newest_first)
        });
        Ok(result)
    }
}
