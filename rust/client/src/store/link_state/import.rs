//! Apply one exact private-state prefix, atomically with visibility and import progress.
use super::{load_hlc, progress, save_hlc, Error, LinkState, Store};
use prost::Message as _;
use rusqlite::{params, Connection, Transaction, TransactionBehavior};

impl Store {
    /// Restore a fresh linked-device snapshot, or replay its exact completed import without writes.
    /// A pending link must match the supplied bytes and its recorded initial/applied phase.
    /// Local and synchronized changes after a successful import are retained across retries.
    ///
    /// # Errors
    /// Returns invalid/corrupt state or progress, mismatched pending bytes, unsupported legacy
    /// private-state recovery, clock overflow or database failures. Import, visibility, clock
    /// and progress roll back together; pending state remains until full link completion.
    pub fn import_link_state(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let state = LinkState::decode(bytes).map_err(|_| Error::InvalidSetting)?;
        let transaction = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        let pending = self.link_pending()?;
        if pending.as_deref().is_some_and(|pending| pending != bytes) {
            return Err(Error::InvalidSetting);
        }
        let phase = progress::read(&self.db, bytes)?;
        if pending.is_some() && phase.is_none() {
            return Err(Error::LinkStateReviewRequired);
        }
        if phase == Some(true) {
            transaction.commit()?;
            return Ok(());
        }
        if pending.is_none() && phase == Some(false) {
            return Err(Error::InvalidSetting);
        }
        write_peers(&self.db, &state)?;
        write_reactions(&self.db, &state)?;
        let clock = imported_clock(&state).max(load_hlc(&self.db)?.into());
        save_hlc(&transaction, clock.into())?;
        self.set_profile_hidden(state.hidden)?;
        progress::save(&self.db, bytes, true)?;
        transaction.commit()?;
        Ok(())
    }
}

fn imported_clock(state: &LinkState) -> u64 {
    let rows = state.likes.iter().map(|like| like.hlc);
    let peers = state.peers.iter();
    let newest = rows
        .chain(state.passes.iter().map(|pass| pass.updated_hlc))
        .chain(peers.flat_map(|peer| {
            [
                peer.blocked_hlc,
                peer.muted_hlc,
                peer.name_hlc,
                peer.favorite_hlc,
                peer.note_hlc,
            ]
        }))
        .max()
        .and_then(|row| u64::try_from(row).ok())
        .unwrap_or(0);
    newest.max(state.hlc)
}

fn write_peers(tx: &Connection, state: &LinkState) -> Result<(), Error> {
    for peer in &state.peers {
        if !crate::chat::valid_note(&peer.note) {
            return Err(Error::InvalidSetting);
        }
        tx.execute(
            "INSERT OR REPLACE INTO `peers` (`peer_id`, `blocked`, `muted`, `local_name`,
                 `seen_devices`, `seen_version`, `verified`, `new_device`, `blocked_hlc`,
                 `muted_hlc`, `name_hlc`, `favorite`, `favorite_hlc`,
                 `blocked_device`, `muted_device`, `name_device`, `favorite_device`, `note`, `note_hlc`, `note_device`)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                peer.peer_id,
                peer.blocked,
                peer.muted,
                peer.local_name,
                peer.seen_devices,
                peer.seen_version,
                peer.verified,
                peer.new_device,
                peer.blocked_hlc,
                peer.muted_hlc,
                peer.name_hlc,
                peer.favorite,
                peer.favorite_hlc,
                peer.blocked_device,
                peer.muted_device,
                peer.name_device,
                peer.favorite_device,
                peer.note,
                peer.note_hlc,
                peer.note_device
            ],
        )?;
    }
    Ok(())
}

fn write_reactions(tx: &Connection, state: &LinkState) -> Result<(), Error> {
    for like in &state.likes {
        tx.execute(
            "INSERT OR REPLACE INTO `like_state`
                 (`sender`, `target`, `liked`, `hlc`, `device`, `event_id`)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                like.sender,
                like.target,
                like.liked,
                like.hlc,
                like.device,
                like.event_id
            ],
        )?;
    }
    for pass in &state.passes {
        tx.execute(
            "INSERT OR REPLACE INTO `pass_history` (`peer_id`, `passed`, `updated_hlc`, `updated_device`)
             VALUES (?1, ?2, ?3, ?4)",
            params![pass.peer_id, pass.passed, pass.updated_hlc, pass.updated_device],
        )?;
    }
    Ok(())
}
