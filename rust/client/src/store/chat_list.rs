//! Pair-wide Chats list projection from local conversations and current message rows.

use super::messages::{ChatMessage, CurrentMessage, MessageKey};
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
    /// Incoming current messages not yet viewed on this device.
    pub unread_count: usize,
}

impl Store {
    /// Read active conversations, latest current messages and local unread counts.
    /// Timed rows sort first by original HLC; legacy rows use local arrival order. Empty
    /// conversations follow rows with messages, then Peer ID breaks any remaining tie.
    pub fn chat_rows(&self) -> Result<Vec<ChatRow>, Error> {
        let mut query = self.db.prepare(
            "WITH `ranked` AS (
                SELECT `g`.`peer_id`, `m`.`group_id`, `m`.`author`, `m`.`message_id`,
                       `m`.`text`, `m`.`edited`, `m`.`deleted`, `m`.`original_hlc`,
                       `m`.`unread`, `m`.`arrival_order`,
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
                   COALESCE(`r`.`unread_count`, 0)
            FROM `conversations` AS `c`
            LEFT JOIN `ranked` AS `r`
              ON `c`.`peer_id` = `r`.`peer_id` AND `r`.`message_rank` = 1
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
                row.get::<_, i64>(11)?,
            ))
        })?;
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
                _ => return Err(Error::InboxConflict),
            };
            let last_message = if let Some(group) = group {
                let author = author.ok_or(Error::InboxConflict)?;
                if author != self.owner_peer_id && author != peer_id {
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
                })
            } else {
                None
            };
            result.push(ChatRow {
                peer_id,
                state,
                last_message,
                unread_count: usize::try_from(count).map_err(|_| Error::InboxConflict)?,
            });
        }
        Ok(result)
    }
}
