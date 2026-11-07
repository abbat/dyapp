//! An answer is metadata for an outstanding request, never permission inferred from the wire alone.
use super::ready;
use crate::chat::pack_control::PackAnswer;
use crate::store::{messages::VerifiedChatSender, Error, InboxKey, InboxKind, Store};
use rusqlite::{params, OptionalExtension};

fn decode(bytes: &[u8], expected: &[u8; 32]) -> Result<PackAnswer, Error> {
    let answer = PackAnswer::decode(bytes).map_err(|_| Error::InboxConflict)?;
    if answer.pack_id != *expected || answer.encode().map_err(|_| Error::InboxConflict)? != bytes {
        return Err(Error::InboxConflict);
    }
    Ok(answer)
}

impl Store {
    /// ACK unsolicited/policy-rejected answers without storing a descriptor or initiating a fetch.
    /// A newly accepted answer commits with MLS, inbox digest and ACK; a later answer cannot replace it.
    pub fn stage_sticker_pack_answer(
        &mut self,
        sealed: &[u8],
        inbox: &InboxKey,
        sender: VerifiedChatSender<'_>,
        answer: &PackAnswer,
    ) -> Result<bool, Error> {
        if sender.device.len() != 32 || sender.author.is_empty() {
            return Err(Error::InboxConflict);
        }
        let bytes = answer.encode().map_err(|_| Error::InboxConflict)?;
        let mut tagged = vec![16]; // Planned outer tag; admission is still disabled in Body/Session.
        tagged.extend_from_slice(&bytes);
        let digest = dyapp_identity::sha256(&tagged);
        let owner = self.owner_peer_id.clone();
        let mut accepted = false;
        let inserted = self.stage_inbound_with(
            sealed, inbox, InboxKind::StateChange, &digest, None, |tx| {
                if sender.author == owner { return Ok(()); }
                match ready(tx, &owner, sender.group, sender.author) {
                    Ok(()) => {}
                    Err(Error::BlockedPeer | Error::ChatClosed | Error::GroupMismatch) => return Ok(()),
                    Err(error) => return Err(error),
                }
                let stored: Option<Option<Vec<u8>>> = tx.query_row(
                    "SELECT `answer` FROM `sticker_pack_requests` WHERE `peer_id` = ?1 AND `pack_id` = ?2",
                    params![sender.author, answer.pack_id.as_slice()], |row| row.get(0)).optional()?;
                match stored {
                    None => return Ok(()),
                    Some(Some(previous)) => {
                        decode(&previous, &answer.pack_id)?;
                        return Ok(()); // First admitted descriptor survives replay or competing answers.
                    }
                    Some(None) => {}
                }
                tx.execute(
                    "UPDATE `sticker_pack_requests` SET `answer` = ?3 WHERE `peer_id` = ?1 AND `pack_id` = ?2",
                    params![sender.author, answer.pack_id.as_slice(), &bytes])?;
                accepted = true;
                Ok(())
            })?;
        Ok(inserted && accepted)
    }

    /// Read a descriptor only for the same active request generation in an open/unblocked chat.
    /// The caller must validate the downloaded pack ID and clicked hash before saving.
    pub fn sticker_pack_answer(
        &self,
        group: &[u8],
        peer: &str,
        pack: &[u8; 32],
        token: &[u8; 16],
    ) -> Result<Option<PackAnswer>, Error> {
        ready(&self.db, &self.owner_peer_id, group, peer)?;
        let stored: Option<Option<Vec<u8>>> = self.db.query_row(
            "SELECT `answer` FROM `sticker_pack_requests` WHERE `peer_id` = ?1 AND `pack_id` = ?2 AND `request_token` = ?3",
            params![peer, pack.as_slice(), token.as_slice()], |row| row.get(0)).optional()?;
        stored
            .flatten()
            .map(|bytes| decode(&bytes, pack))
            .transpose()
    }
}

#[cfg(test)]
mod tests;
