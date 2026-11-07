//! A clicked received sticker binds pack intent to the same commit as its exact mailbox writes.
use crate::chat::{pack_control::PackRequest, sticker::Sticker};
use crate::store::{conversations, messages::MessageKey, Conversation, Error, Store};
use crate::PreparedEnvelope;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

/// One request generation. Keep transport descriptors out of screen records.
#[derive(Clone, PartialEq, Eq)]
pub struct PendingPack {
    /// The clicked sticker must occur in the validated returned pack.
    pub sticker_hash: [u8; 32],
    /// Compare before clearing intent so an old completion cannot remove a new request.
    pub token: [u8; 16],
}

pub(super) fn ready(db: &Connection, owner: &str, group: &[u8], peer: &str) -> Result<(), Error> {
    if peer.is_empty() || peer == owner {
        return Err(Error::InboxConflict);
    }
    let catalog: Option<String> = db
        .query_row(
            "SELECT `peer_id` FROM `conversation_groups` WHERE `group_id` = ?",
            [group],
            |row| row.get(0),
        )
        .optional()?;
    if catalog.as_deref() != Some(peer) {
        return Err(Error::GroupMismatch);
    }
    let blocked: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM `peers` WHERE `peer_id` = ? AND `blocked` = 1)",
        [peer],
        |row| row.get(0),
    )?;
    if blocked {
        return Err(Error::BlockedPeer);
    }
    if conversations::load(db, peer)? != Conversation::Open {
        return Err(Error::ChatClosed);
    }
    Ok(())
}

fn clicked(db: &Connection, key: &MessageKey) -> Result<Sticker, Error> {
    let stored: Option<(Vec<u8>, i64)> = db
        .query_row(
            "SELECT `a`.`descriptor`, `a`.`original_hlc` FROM `chat_attachments` AS `a`
         JOIN `conversation_groups` AS `g` ON `g`.`group_id` = `a`.`group_id`
         WHERE `a`.`group_id` = ?1 AND `a`.`author` = ?2 AND `g`.`peer_id` = ?2
           AND `a`.`message_id` = ?3 AND `a`.`media_kind` = 2 AND `a`.`forwarded` = 0
           AND `a`.`delivery_state` IS NULL AND `a`.`delivery_generation` IS NULL",
            params![&key.group, &key.author, key.id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (bytes, hlc) = stored.ok_or(Error::InboxConflict)?;
    let sticker = Sticker::decode(&bytes).map_err(|_| Error::InboxConflict)?;
    if sticker.id != key.id
        || i64::try_from(sticker.hlc).ok() != Some(hlc)
        || sticker.encode().map_err(|_| Error::InboxConflict)? != bytes
    {
        return Err(Error::InboxConflict);
    }
    Ok(sticker)
}

fn pending(db: &Connection, peer: &str, pack: &[u8; 32]) -> Result<Option<PendingPack>, Error> {
    let row: Option<(Vec<u8>, Vec<u8>)> = db.query_row(
        "SELECT `sticker_hash`, `request_token` FROM `sticker_pack_requests` WHERE `peer_id` = ?1 AND `pack_id` = ?2",
        params![peer, pack.as_slice()], |row| Ok((row.get(0)?, row.get(1)?))).optional()?;
    row.map(|(hash, token)| {
        Ok(PendingPack {
            sticker_hash: hash.try_into().map_err(|_| Error::InboxConflict)?,
            token: token.try_into().map_err(|_| Error::InboxConflict)?,
        })
    })
    .transpose()
}

impl Store {
    /// Prepare metadata for the exact received sticker selected in an open pair chat.
    pub fn sticker_pack_request(
        &self,
        group: &[u8],
        selected: &MessageKey,
    ) -> Result<PackRequest, Error> {
        ready(&self.db, &self.owner_peer_id, group, &selected.author)?;
        Ok(PackRequest {
            pack_id: clicked(&self.db, selected)?.pack_id,
        })
    }

    /// Commit pending intent, MLS snapshot and request outbox together; bounded to 64 per identity.
    pub fn stage_sticker_pack_request(
        &mut self,
        sealed: &[u8],
        writes: &[PreparedEnvelope],
        group: &[u8],
        selected: &MessageKey,
        request: &PackRequest,
    ) -> Result<PendingPack, Error> {
        let owner = self.owner_peer_id.clone();
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ready(&tx, &owner, group, &selected.author)?;
        let sticker = clicked(&tx, selected)?;
        if sticker.pack_id != request.pack_id {
            return Err(Error::InboxConflict);
        }
        let pending = if let Some(pending) = pending(&tx, &selected.author, &request.pack_id)? {
            pending
        } else {
            let count: i64 =
                tx.query_row("SELECT COUNT(*) FROM `sticker_pack_requests`", [], |row| {
                    row.get(0)
                })?;
            if count >= 64 {
                return Err(Error::PackRequestsFull);
            }
            let mut token = [0; 16];
            getrandom::fill(&mut token).map_err(|_| Error::Random)?;
            tx.execute(
                "INSERT INTO `sticker_pack_requests` (`peer_id`, `pack_id`, `sticker_hash`, `request_token`)
                 VALUES (?1, ?2, ?3, ?4)",
                params![&selected.author, request.pack_id.as_slice(), sticker.sha256.as_slice(), token.as_slice()])?;
            PendingPack {
                sticker_hash: sticker.sha256,
                token,
            }
        };
        Self::stage_outbound_tx(&tx, None, sealed, writes, true)?;
        for write in writes {
            let changed = tx.execute(
                "UPDATE `outbox` SET `chat_group` = ?3 WHERE `mailbox` = ?1 AND `envelope_id` = ?2
                 AND (`chat_group` IS NULL OR `chat_group` = ?3)",
                params![write.mailbox().as_slice(), write.id().as_slice(), group],
            )?;
            if changed != 1 {
                return Err(Error::OutboxConflict);
            }
        }
        tx.commit()?;
        Ok(pending)
    }

    /// Read identity-local intent; absence never authorizes accepting a peer answer.
    pub fn pending_sticker_pack(
        &self,
        peer: &str,
        pack: &[u8; 32],
    ) -> Result<Option<PendingPack>, Error> {
        pending(&self.db, peer, pack)
    }

    /// Retire acceptance intent after saving a validated pack, only for the observed generation.
    /// This does not revoke mailbox writes already queued for delivery.
    pub fn finish_sticker_pack(
        &self,
        peer: &str,
        pack: &[u8; 32],
        token: &[u8; 16],
    ) -> Result<bool, Error> {
        Ok(self.db.execute(
            "DELETE FROM `sticker_pack_requests` WHERE `peer_id` = ?1 AND `pack_id` = ?2 AND `request_token` = ?3",
            params![peer, pack.as_slice(), token.as_slice()])? == 1)
    }
}

mod answer;
#[cfg(test)]
mod tests;
