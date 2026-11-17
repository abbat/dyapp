//! Pair-wide Chats list projection from local conversations and current message rows.

use super::messages::attachments::ItemOrder;
use super::messages::{
    ChatAttachment, ChatMessage, ChatSticker, CurrentMessage, DeliveryState, MessageKey,
};
use super::{Conversation, Error, Store};
use dyapp_messaging::hlc::Timestamp;
use std::collections::HashMap;

const CHAT_ROWS_QUERY: &str = "WITH `ranked` AS (
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
         `r`.`message_id` DESC, `r`.`group_id` DESC, `c`.`peer_id`";

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

struct StoredMessage {
    group: Option<Vec<u8>>,
    author: Option<String>,
    id: Option<Vec<u8>>,
    text: Option<Vec<u8>>,
    edited: Option<i64>,
    deleted: Option<i64>,
    time: Option<i64>,
    unread: Option<i64>,
    delivery: Option<i64>,
    forwarded: Option<i64>,
}

struct StoredChatRow {
    peer_id: String,
    state: i64,
    request: Option<Vec<u8>>,
    message: StoredMessage,
    count: i64,
}

struct DecodedChatRow {
    peer_id: String,
    state: Conversation,
    last_message: Option<ChatMessage>,
    unread_texts: usize,
}

impl StoredChatRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            peer_id: row.get(0)?,
            state: row.get(1)?,
            request: row.get(2)?,
            message: StoredMessage {
                group: row.get(3)?,
                author: row.get(4)?,
                id: row.get(5)?,
                text: row.get(6)?,
                edited: row.get(7)?,
                deleted: row.get(8)?,
                time: row.get(9)?,
                unread: row.get(10)?,
                delivery: row.get(11)?,
                forwarded: row.get(12)?,
            },
            count: row.get(13)?,
        })
    }

    fn decode(self, owner: &str) -> Result<DecodedChatRow, Error> {
        let state = stored_conversation(self.state, self.request.as_deref())?;
        let last_message = self.message.decode(owner, &self.peer_id)?;
        let unread_texts = usize::try_from(self.count).map_err(|_| Error::InboxConflict)?;
        Ok(DecodedChatRow {
            peer_id: self.peer_id,
            state,
            last_message,
            unread_texts,
        })
    }
}

fn stored_conversation(state: i64, request: Option<&[u8]>) -> Result<Conversation, Error> {
    let request_id = || -> Result<[u8; 16], Error> {
        request
            .and_then(|id| id.try_into().ok())
            .ok_or(Error::InboxConflict)
    };
    match state {
        1 => Ok(Conversation::RequestSent(request_id()?)),
        2 => Ok(Conversation::RequestReceived(request_id()?)),
        3 if request.is_none() => Ok(Conversation::Open),
        4 if request.is_none() => Ok(Conversation::Closed),
        _ => Err(Error::InboxConflict),
    }
}

impl StoredMessage {
    fn decode(self, owner: &str, peer_id: &str) -> Result<Option<ChatMessage>, Error> {
        let Some(group) = self.group else {
            return Ok(None);
        };
        let author = self.author.ok_or(Error::InboxConflict)?;
        if author != owner && author != peer_id {
            return Err(Error::InboxConflict);
        }
        let delivery = DeliveryState::from_stored(self.delivery)?;
        if author != owner && delivery.is_some() {
            return Err(Error::InboxConflict);
        }
        let id = self
            .id
            .ok_or(Error::InboxConflict)?
            .try_into()
            .map_err(|_| Error::InboxConflict)?;
        let current = CurrentMessage::from_stored(
            self.text,
            self.edited.ok_or(Error::InboxConflict)?,
            self.deleted.ok_or(Error::InboxConflict)?,
        )?;
        let original_hlc = self
            .time
            .map(|value| {
                u64::try_from(value)
                    .map(Timestamp::from)
                    .map_err(|_| Error::InboxConflict)
            })
            .transpose()?;
        let unread = match self.unread {
            Some(0) => false,
            Some(1) => true,
            _ => return Err(Error::InboxConflict),
        };
        Ok(Some(ChatMessage {
            key: MessageKey { group, author, id },
            current,
            original_hlc,
            unread,
            forwarded: self.forwarded == Some(1),
            delivery,
        }))
    }
}

fn with_media(
    decoded: DecodedChatRow,
    attachments: &mut HashMap<String, (ChatAttachment, usize)>,
    stickers: &mut HashMap<String, (ChatSticker, usize)>,
) -> ChatRow {
    let DecodedChatRow {
        peer_id,
        state,
        last_message,
        unread_texts,
    } = decoded;
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
    ChatRow {
        peer_id,
        state,
        last_message,
        last_attachment,
        last_sticker,
        unread_count: unread_texts + unread_attachments + unread_stickers,
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
        let mut query = self.db.prepare(CHAT_ROWS_QUERY)?;
        let rows = query.query_map([], StoredChatRow::from_row)?;
        let mut attachments = self.latest_chat_attachments()?;
        let mut stickers = self.latest_chat_stickers()?;
        let mut result = Vec::new();
        for row in rows {
            let decoded = row?.decode(&self.owner_peer_id)?;
            result.push(with_media(decoded, &mut attachments, &mut stickers));
        }
        // The sort is stable, so legacy and empty rows keep the order the query gave them.
        result.sort_by(|a, b| {
            let newest_first = || b.latest().cmp(&a.latest());
            a.group().cmp(&b.group()).then_with(newest_first)
        });
        Ok(result)
    }
}
