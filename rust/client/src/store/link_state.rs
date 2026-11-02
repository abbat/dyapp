//! The owner's local state a linked device starts from: blocks, mutes, names and verified marks,
//! likes, passes and whether the profile is hidden. Later changes reach it as own-device sync.
use super::likes::{load_hlc, save_hlc};
use super::{Error, Store};
use prost::Message as _;
use rusqlite::params;

#[derive(Clone, PartialEq, prost::Message)]
struct LinkState {
    #[prost(message, repeated, tag = "1")]
    peers: Vec<Peer>,
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
struct Peer {
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
}

impl Store {
    /// This store's local state as the bytes of a link record.
    pub fn link_state(&self) -> Result<Vec<u8>, Error> {
        let mut peers = self.db.prepare(
            "SELECT `peer_id`, `blocked`, `muted`, `local_name`, `seen_devices`, `seen_version`,
                    `verified`, `new_device`, `blocked_hlc`, `muted_hlc`, `name_hlc`
             FROM `peers` ORDER BY `peer_id`",
        )?;
        let peers = peers.query_map([], |row| {
            Ok(Peer {
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
            "SELECT `peer_id`, `passed`, `updated_hlc` FROM `pass_history` ORDER BY `peer_id`",
        )?;
        let passes = passes.query_map([], |row| {
            Ok(Pass {
                peer_id: row.get(0)?,
                passed: row.get(1)?,
                updated_hlc: row.get(2)?,
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
    pub fn import_link_state(&mut self, state: &[u8]) -> Result<(), Error> {
        let state = LinkState::decode(state).map_err(|_| Error::InvalidSetting)?;
        let rows = state.likes.iter().map(|like| like.hlc);
        let peers = state.peers.iter();
        let newest = rows
            .chain(state.passes.iter().map(|pass| pass.updated_hlc))
            .chain(peers.flat_map(|peer| [peer.blocked_hlc, peer.muted_hlc, peer.name_hlc]))
            .max()
            .and_then(|row| u64::try_from(row).ok())
            .unwrap_or(0);
        let clock = newest.max(state.hlc).max(load_hlc(&self.db)?.into());
        let tx = self.db.transaction()?;
        for peer in &state.peers {
            tx.execute(
                "INSERT OR REPLACE INTO `peers` (`peer_id`, `blocked`, `muted`, `local_name`,
                     `seen_devices`, `seen_version`, `verified`, `new_device`, `blocked_hlc`,
                     `muted_hlc`, `name_hlc`)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
                    peer.name_hlc
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
                "INSERT OR REPLACE INTO `pass_history` (`peer_id`, `passed`, `updated_hlc`)
                 VALUES (?1, ?2, ?3)",
                params![pass.peer_id, pass.passed, pass.updated_hlc],
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
        assert!(clock >= rows as u64);
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
        assert!(store.blocked_peers().unwrap().is_empty());
    }
}
