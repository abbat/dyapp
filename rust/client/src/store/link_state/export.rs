//! Stable ordered private-state snapshot for a linked device.
use super::{load_hlc, Error, Like, LinkState, Pass, PeerSnapshot, Store};
use prost::Message as _;
use rusqlite::{Connection, Transaction, TransactionBehavior};

impl Store {
    /// This store's local state as the bytes of a link record, from one read snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or decoding peer, like, pass, hybrid-clock, or
    /// visibility state fails.
    pub fn link_state(&self) -> Result<Vec<u8>, Error> {
        let snapshot = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let bytes = LinkState {
            peers: peers(&self.db)?,
            likes: likes(&self.db)?,
            passes: passes(&self.db)?,
            hlc: load_hlc(&self.db)?.into(),
            hidden: self.profile_hidden()?,
        }
        .encode_to_vec();
        snapshot.commit()?;
        Ok(bytes)
    }
}

fn peers(db: &Connection) -> Result<Vec<PeerSnapshot>, Error> {
    let mut peers = db.prepare(
        "SELECT `peer_id`, `blocked`, `muted`, `local_name`, `seen_devices`, `seen_version`,
                `verified`, `new_device`, `blocked_hlc`, `muted_hlc`, `name_hlc`,
                `favorite`, `favorite_hlc`, `blocked_device`, `muted_device`, `name_device`, `favorite_device`,
                `note`, `note_hlc`, `note_device`
         FROM `peers` ORDER BY `peer_id`",
    )?;
    let peers = peers.query_map([], |row| {
        Ok(PeerSnapshot {
            peer_id: row.get(0)?,
            blocked: row.get(1)?,
            muted: row.get(2)?,
            local_name: row.get(3)?,
            seen_devices: row.get(4)?,
            seen_version: row.get(5)?,
            verified: row.get(6)?,
            new_device: row.get(7)?,
            blocked_hlc: row.get(8)?,
            muted_hlc: row.get(9)?,
            name_hlc: row.get(10)?,
            favorite: row.get(11)?,
            favorite_hlc: row.get(12)?,
            blocked_device: row.get(13)?,
            muted_device: row.get(14)?,
            name_device: row.get(15)?,
            favorite_device: row.get(16)?,
            note: row.get(17)?,
            note_hlc: row.get(18)?,
            note_device: row.get(19)?,
        })
    })?;
    Ok(peers.collect::<Result<_, _>>()?)
}

fn likes(db: &Connection) -> Result<Vec<Like>, Error> {
    let mut likes = db.prepare(
        "SELECT `sender`, `target`, `liked`, `hlc`, `device`, `event_id`
         FROM `like_state` ORDER BY `sender`, `target`",
    )?;
    let likes = likes.query_map([], |row| {
        Ok(Like {
            sender: row.get(0)?,
            target: row.get(1)?,
            liked: row.get(2)?,
            hlc: row.get(3)?,
            device: row.get(4)?,
            event_id: row.get(5)?,
        })
    })?;
    Ok(likes.collect::<Result<_, _>>()?)
}

fn passes(db: &Connection) -> Result<Vec<Pass>, Error> {
    let mut passes = db.prepare(
        "SELECT `peer_id`, `passed`, `updated_hlc`, `updated_device` FROM `pass_history` ORDER BY `peer_id`",
    )?;
    let passes = passes.query_map([], |row| {
        Ok(Pass {
            peer_id: row.get(0)?,
            passed: row.get(1)?,
            updated_hlc: row.get(2)?,
            updated_device: row.get(3)?,
        })
    })?;
    Ok(passes.collect::<Result<_, _>>()?)
}
