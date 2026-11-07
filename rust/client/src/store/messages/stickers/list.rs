//! One validated newest sticker per peer, with counts across every sticker row.

use super::{ChatSticker, Error, Store};
use crate::store::messages::attachments::stored_media;
use std::collections::HashMap;

impl Store {
    pub(crate) fn latest_chat_stickers(
        &self,
    ) -> Result<HashMap<String, (ChatSticker, usize)>, Error> {
        let mut query = self.db.prepare(
            "SELECT `group_id`, `author`, `message_id`, `original_hlc`, `descriptor`, `unread`,
                    `forwarded`, `delivery_state`, `delivery_generation`, `peer_id`, `unread_count`
             FROM (
                 SELECT `a`.*, `g`.`peer_id`,
                        ROW_NUMBER() OVER (
                            PARTITION BY `g`.`peer_id`
                            ORDER BY `a`.`original_hlc` DESC, `a`.`author` DESC,
                                     `a`.`message_id` DESC, `a`.`group_id` DESC
                        ) AS `sticker_rank`,
                        SUM(`a`.`unread`) OVER (PARTITION BY `g`.`peer_id`) AS `unread_count`
                 FROM `chat_attachments` AS `a`
                 JOIN `conversation_groups` AS `g` ON `a`.`group_id` = `g`.`group_id`
                 WHERE `a`.`media_kind` = 2
             )
             WHERE `sticker_rank` = 1",
        )?;
        let rows = query.query_map([], |row| {
            Ok((
                stored_media(row)?,
                row.get::<_, String>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })?;
        rows.map(|row| {
            let (stored, peer, count) = row?;
            let sticker = self.sticker_from(&peer, stored)?;
            let count = usize::try_from(count).map_err(|_| Error::InboxConflict)?;
            Ok((peer, (sticker, count)))
        })
        .collect()
    }
}
