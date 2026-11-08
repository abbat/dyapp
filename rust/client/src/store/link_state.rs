//! The owner's local state a linked device starts from: blocks, mutes, favorites, names and verified marks,
//! likes, passes and whether the profile is hidden. Later changes reach it as own-device sync.
use super::likes::{load_hlc, save_hlc};
use super::{Error, Store};
use prost::Message as _;
use rusqlite::{params, OptionalExtension};

/// The `settings` row of a record's state until the linked device has listed itself.
const PENDING: &str = "link_pending";
/// The `settings` row of a device that joined by a link. It outlives the link: chats here start
/// at that moment, for good.
const LINKED: &str = "linked_device";

#[derive(Clone, PartialEq, prost::Message)]
struct LinkState {
    #[prost(message, repeated, tag = "1")]
    peers: Vec<PeerSnapshot>,
    #[prost(message, repeated, tag = "2")]
    likes: Vec<Like>,
    #[prost(message, repeated, tag = "3")]
    passes: Vec<Pass>,
    #[prost(uint64, tag = "4")]
    hlc: u64,
    #[prost(bool, tag = "5")]
    hidden: bool,
}

#[derive(Clone, PartialEq, prost::Message)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent peer settings retain their separate protobuf tags and merge clocks"
)]
struct PeerSnapshot {
    #[prost(string, tag = "1")]
    peer_id: String,
    #[prost(bool, tag = "2")]
    blocked: bool,
    #[prost(bool, tag = "3")]
    muted: bool,
    #[prost(string, tag = "4")]
    local_name: String,
    #[prost(bytes = "vec", optional, tag = "5")]
    seen_devices: Option<Vec<u8>>,
    #[prost(int64, optional, tag = "6")]
    seen_version: Option<i64>,
    #[prost(bool, tag = "7")]
    verified: bool,
    #[prost(bool, tag = "8")]
    new_device: bool,
    #[prost(int64, tag = "9")]
    blocked_hlc: i64,
    #[prost(int64, tag = "10")]
    muted_hlc: i64,
    #[prost(int64, tag = "11")]
    name_hlc: i64,
    #[prost(bool, tag = "12")]
    favorite: bool,
    #[prost(int64, tag = "13")]
    favorite_hlc: i64,
    #[prost(bytes = "vec", tag = "14")]
    blocked_device: Vec<u8>,
    #[prost(bytes = "vec", tag = "15")]
    muted_device: Vec<u8>,
    #[prost(bytes = "vec", tag = "16")]
    name_device: Vec<u8>,
    #[prost(bytes = "vec", tag = "17")]
    favorite_device: Vec<u8>,
    #[prost(string, tag = "18")]
    note: String,
    #[prost(int64, tag = "19")]
    note_hlc: i64,
    #[prost(bytes = "vec", tag = "20")]
    note_device: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct Like {
    #[prost(string, tag = "1")]
    sender: String,
    #[prost(string, tag = "2")]
    target: String,
    #[prost(bool, tag = "3")]
    liked: bool,
    #[prost(int64, tag = "4")]
    hlc: i64,
    #[prost(bytes = "vec", tag = "5")]
    device: Vec<u8>,
    #[prost(bytes = "vec", tag = "6")]
    event_id: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct Pass {
    #[prost(string, tag = "1")]
    peer_id: String,
    #[prost(bool, tag = "2")]
    passed: bool,
    #[prost(int64, tag = "3")]
    updated_hlc: i64,
    #[prost(bytes = "vec", tag = "4")]
    updated_device: Vec<u8>,
}

impl Store {
    /// Keeps a received record's `state` until [`crate::session::Session::finish_link`] ends: the
    /// mailbox no longer holds the record, so a failed step must not lose it.
    ///
    /// # Errors
    ///
    /// Returns an error if either database write fails. The linked-device marker
    /// may already be stored when saving pending state fails.
    pub fn set_link_pending(&self, state: &[u8]) -> Result<(), Error> {
        self.db.execute(
            "INSERT OR IGNORE INTO `settings` (`key`, `format_version`, `value`)
             VALUES (?1, 1, x'')",
            [LINKED],
        )?;
        self.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = 1, `value` = excluded.`value`",
            params![PENDING, state],
        )?;
        Ok(())
    }

    /// The state of a link this device has not finished.
    ///
    /// # Errors
    ///
    /// Returns an error if querying or decoding the pending row fails, or its
    /// format version is unsupported.
    pub fn link_pending(&self) -> Result<Option<Vec<u8>>, Error> {
        let row: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT `format_version`, `value` FROM `settings` WHERE `key` = ?1",
                [PENDING],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((1, state)) => Ok(Some(state)),
            Some(_) => Err(Error::InvalidSetting),
        }
    }

    /// Whether this device joined by a link, so holds no history from before that.
    ///
    /// # Errors
    ///
    /// Returns an error if querying or decoding the linked-device marker fails.
    pub fn is_linked_device(&self) -> Result<bool, Error> {
        Ok(self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM `settings` WHERE `key` = ?1)",
            [LINKED],
            |row| row.get(0),
        )?)
    }

    /// Ends a link: the state now lives in the tables it was imported to.
    ///
    /// # Errors
    ///
    /// Returns an error if deleting the pending-state row from the database fails.
    pub fn clear_link_pending(&self) -> Result<(), Error> {
        self.db
            .execute("DELETE FROM `settings` WHERE `key` = ?1", [PENDING])?;
        Ok(())
    }

    /// This store's local state as the bytes of a link record.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or decoding peer, like, pass, hybrid-clock, or
    /// visibility state fails.
    pub fn link_state(&self) -> Result<Vec<u8>, Error> {
        let mut peers = self.db.prepare(
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
        let mut likes = self.db.prepare(
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
        let mut passes = self.db.prepare(
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
        let state = LinkState {
            peers: peers.collect::<Result<_, _>>()?,
            likes: likes.collect::<Result<_, _>>()?,
            passes: passes.collect::<Result<_, _>>()?,
            hlc: load_hlc(&self.db)?.into(),
            hidden: self.profile_hidden()?,
        };
        Ok(state.encode_to_vec())
    }

    /// Restores [`Self::link_state`] on the fresh store of a linked device; a retry changes
    /// nothing. The clock goes past every row, or later events would lose to them.
    ///
    /// # Errors
    ///
    /// Returns an error if the snapshot cannot be decoded, a peer note is invalid,
    /// stored clock state is invalid or outside its supported range, or database
    /// access or transaction commit fails. Imported rows and the clock are already
    /// committed if the final visibility-setting write fails.
    pub fn import_link_state(&mut self, state: &[u8]) -> Result<(), Error> {
        let state = LinkState::decode(state).map_err(|_| Error::InvalidSetting)?;
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
        let clock = newest.max(state.hlc).max(load_hlc(&self.db)?.into());
        let tx = self.db.transaction()?;
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
        save_hlc(&tx, clock.into())?;
        tx.commit()?;
        self.set_profile_hidden(state.hidden)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::temp_db_path;
    use super::*;
    use crate::interactions::Kind;
    use dyapp_identity::Identity;

    #[test]
    fn a_pending_link_stays_until_cleared_and_reads_only_its_format() {
        let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        assert_eq!(store.link_pending().unwrap(), None);
        assert!(!store.is_linked_device().unwrap());
        store.set_link_pending(b"first").unwrap();
        store.set_link_pending(b"state").unwrap();
        assert_eq!(store.link_pending().unwrap().unwrap(), b"state");
        store
            .db
            .execute(
                "UPDATE `settings` SET `format_version` = 2 WHERE `key` = ?1",
                [PENDING],
            )
            .unwrap();
        assert!(matches!(store.link_pending(), Err(Error::InvalidSetting)));
        store.clear_link_pending().unwrap();
        assert_eq!(store.link_pending().unwrap(), None);
        assert!(store.is_linked_device().unwrap());
    }

    #[test]
    fn linked_notes_keep_text_clock_actor_and_reject_invalid_snapshots() {
        use crate::chat::PeerChange;
        let owner = Identity::generate();
        let old = Store::open(&temp_db_path(), &owner).unwrap();
        let high = 1u64 << 40;
        old.apply_peer_change(
            "bob",
            &PeerChange::Note("Case\nLine".into()),
            high.into(),
            &[2; 32],
        )
        .unwrap();
        let snapshot = old.link_state().unwrap();
        let mut linked = Store::open(&temp_db_path(), &owner).unwrap();
        linked.import_link_state(&snapshot).unwrap();
        assert_eq!(linked.contact_note("bob").unwrap(), "Case\nLine");
        assert!(u64::from(linked.tick(0).unwrap()) > high);
        assert!(!linked
            .receive_peer_change(
                "bob",
                &PeerChange::Note("older actor".into()),
                high,
                dyapp_messaging::hlc::Timestamp::from(high).wall_ms(),
                &[1; 32]
            )
            .unwrap());
        let mut invalid = LinkState::decode(snapshot.as_slice()).unwrap();
        invalid.peers[0].note = "hidden\u{202e}".into();
        assert!(matches!(
            linked.import_link_state(&invalid.encode_to_vec()),
            Err(Error::InvalidSetting)
        ));
        assert_eq!(linked.contact_note("bob").unwrap(), "Case\nLine");
    }

    #[test]
    fn linked_favorites_keep_their_clock_and_hide_blocked_peers() {
        let owner = Identity::generate();
        let old = Store::open(&temp_db_path(), &owner).unwrap();
        old.apply_peer_change(
            "bob",
            &crate::chat::PeerChange::Favorite(true),
            1u64.into(),
            &[],
        )
        .unwrap();
        old.set_blocked("bob", true).unwrap();
        let high = 1i64 << 40;
        old.db
            .execute("UPDATE `peers` SET `favorite_hlc` = ?1", [high])
            .unwrap();
        let mut linked = Store::open(&temp_db_path(), &owner).unwrap();
        linked
            .import_link_state(&old.link_state().unwrap())
            .unwrap();
        assert!(linked.favorite("bob").unwrap());
        assert_eq!(
            linked.favorite_peers().unwrap(),
            [] as [std::string::String; 0]
        );
        assert!(u64::from(linked.tick(0).unwrap()) > u64::try_from(high).unwrap());
        linked.set_blocked("bob", false).unwrap();
        assert_eq!(linked.favorite_peers().unwrap(), ["bob"]);
    }

    #[test]
    fn a_fresh_store_takes_the_owners_state_and_later_events_win() {
        let alice = Identity::generate();
        let mut old = Store::open(&temp_db_path(), &alice).unwrap();
        old.set_blocked("bob", true).unwrap();
        old.set_muted("carol", true).unwrap();
        old.set_local_name("carol", "Carrie").unwrap();
        old.db
            .execute(
                "UPDATE `peers` SET `verified` = 1 WHERE `peer_id` = 'carol'",
                [],
            )
            .unwrap();
        old.interact("dave", Kind::Like, b"phone", 1_000).unwrap();
        old.pass("erin", true, 1_000).unwrap();
        old.set_profile_hidden(true).unwrap();
        // A row can be newer than the local clock, as one from a peer's clock is.
        old.db
            .execute(
                "UPDATE `pass_history` SET `updated_hlc` = `updated_hlc` + 1000000",
                [],
            )
            .unwrap();

        let mut new = Store::open(&temp_db_path(), &alice).unwrap();
        let state = old.link_state().unwrap();
        new.import_link_state(&state).unwrap();
        new.import_link_state(&state).unwrap();

        assert!(new.blocked("bob").unwrap());
        assert!(new.muted("carol").unwrap());
        assert_eq!(new.local_name("carol").unwrap(), "Carrie");
        assert_eq!(new.passed_peers().unwrap(), ["erin"]);
        assert!(new.profile_hidden().unwrap());
        let (rows, clock): (i64, u64) = (
            new.db
                .query_row("SELECT `updated_hlc` FROM `pass_history`", [], |row| {
                    row.get(0)
                })
                .unwrap(),
            load_hlc(&new.db).unwrap().into(),
        );
        assert!(clock >= u64::try_from(rows).unwrap());
        // Peers, likes and passes arrive whole: export again and only the clock may differ.
        let (old_state, new_state) = (
            LinkState::decode(state.as_slice()).unwrap(),
            LinkState::decode(new.link_state().unwrap().as_slice()).unwrap(),
        );
        assert_eq!(
            LinkState {
                hlc: 0,
                ..old_state
            },
            LinkState {
                hlc: 0,
                ..new_state
            }
        );
    }

    #[test]
    fn bytes_that_are_no_state_change_nothing() {
        let alice = Identity::generate();
        let mut store = Store::open(&temp_db_path(), &alice).unwrap();
        assert!(matches!(
            store.import_link_state(&[0xFF]),
            Err(Error::InvalidSetting)
        ));
        assert_eq!(
            store.blocked_peers().unwrap(),
            [] as [std::string::String; 0]
        );
    }
}
