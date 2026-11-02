//! Per-peer conversation state: a pending message request until a reply or a match opens it, and
//! closed once a match ends; see docs/design/chats-and-matches.md.

use super::{Error, Store};
use rusqlite::{params, OptionalExtension, Transaction};

// `state`: 1 request sent, 2 request received, 3 open, 4 closed; no row is none.
pub(super) const SCHEMA: &str = r"
CREATE TABLE `conversations` (
    `peer_id` TEXT PRIMARY KEY CHECK (length(`peer_id`) > 0),
    `state` INTEGER NOT NULL CHECK (`state` IN (1, 2, 3, 4)),
    `request_id` BLOB,
    CHECK ((`state` IN (3, 4) AND `request_id` IS NULL)
        OR (`state` IN (1, 2) AND length(`request_id`) = 16))
) STRICT;
";

/// Where a pair's chat stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversation {
    /// No text and no match yet.
    None,
    /// This profile sent the one pending text with this logical ID.
    RequestSent([u8; 16]),
    /// The peer sent the one pending text with this logical ID.
    RequestReceived([u8; 16]),
    /// A reply or a match opened the chat; only an ended match closes it.
    Open,
    /// A match ended. Nothing more is sent or shown until a new match opens the chat.
    Closed,
}

impl Store {
    /// This profile's conversation state with `peer_id`.
    pub fn conversation(&self, peer_id: &str) -> Result<Conversation, Error> {
        load(&self.db, peer_id)
    }
}

fn load(db: &rusqlite::Connection, peer_id: &str) -> Result<Conversation, Error> {
    let row: Option<(i64, Option<Vec<u8>>)> = db
        .query_row(
            "SELECT `state`, `request_id` FROM `conversations` WHERE `peer_id` = ?1",
            [peer_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let id = |id: Option<Vec<u8>>| {
        id.and_then(|id| id.try_into().ok())
            .ok_or(Error::InboxConflict)
    };
    Ok(match row {
        None => Conversation::None,
        Some((1, request)) => Conversation::RequestSent(id(request)?),
        Some((2, request)) => Conversation::RequestReceived(id(request)?),
        Some((3, None)) => Conversation::Open,
        Some((4, None)) => Conversation::Closed,
        Some(_) => return Err(Error::InboxConflict),
    })
}

fn save(tx: &Transaction<'_>, peer_id: &str, state: Conversation) -> Result<(), Error> {
    let (code, request) = match state {
        Conversation::None => return Ok(()),
        Conversation::RequestSent(id) => (1, Some(id)),
        Conversation::RequestReceived(id) => (2, Some(id)),
        Conversation::Open => (3, None),
        Conversation::Closed => (4, None),
    };
    tx.execute(
        "INSERT INTO `conversations` (`peer_id`, `state`, `request_id`) VALUES (?1, ?2, ?3)
         ON CONFLICT (`peer_id`) DO UPDATE SET `state` = excluded.`state`,
         `request_id` = excluded.`request_id`",
        params![peer_id, code, request.as_ref().map(<[u8; 16]>::as_slice)],
    )?;
    Ok(())
}

pub(super) fn matched(tx: &Transaction<'_>, owner: &str, peer_id: &str) -> Result<bool, Error> {
    Ok(tx.query_row(
        "SELECT COUNT(*) = 2 FROM `like_state` WHERE `liked` = 1
         AND ((`sender` = ?1 AND `target` = ?2) OR (`sender` = ?2 AND `target` = ?1))",
        params![owner, peer_id],
        |row| row.get(0),
    )?)
}

/// The peer a cataloged one-to-one group belongs to, checked against the verified author.
fn group_peer(
    tx: &Transaction<'_>,
    owner: &str,
    group: &[u8],
    author: &str,
) -> Result<String, Error> {
    let peer: String = tx
        .query_row(
            "SELECT `peer_id` FROM `conversation_groups` WHERE `group_id` = ?1",
            [group],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(Error::GroupMismatch)?;
    if author != owner && author != peer {
        return Err(Error::GroupMismatch);
    }
    Ok(peer)
}

/// Apply the request rules to a text with logical `id` before it is recorded. Returns whether to
/// record it; a dropped text is a second pending request from the peer or a text after a match
/// ended. `outbound` refuses a second own request with [`Error::RequestPending`] and a text to a
/// closed chat with [`Error::ChatClosed`]; an own text from another device is kept.
pub(super) fn on_text(
    tx: &Transaction<'_>,
    owner: &str,
    group: &[u8],
    author: &str,
    id: [u8; 16],
    outbound: bool,
) -> Result<bool, Error> {
    let peer = group_peer(tx, owner, group, author)?;
    let own = author == owner;
    let next = match load(tx, &peer)? {
        Conversation::Open => return Ok(true),
        Conversation::Closed if outbound => return Err(Error::ChatClosed),
        Conversation::Closed => return Ok(false),
        _ if matched(tx, owner, &peer)? => Conversation::Open,
        Conversation::None if own => Conversation::RequestSent(id),
        Conversation::None => Conversation::RequestReceived(id),
        Conversation::RequestSent(_) if !own => Conversation::Open,
        Conversation::RequestReceived(_) if own => Conversation::Open,
        Conversation::RequestSent(request) => {
            if outbound && request != id {
                return Err(Error::RequestPending);
            }
            return Ok(true);
        }
        Conversation::RequestReceived(request) => return Ok(request == id),
    };
    save(tx, &peer, next)?;
    Ok(true)
}

/// Attachments can answer a pending text request or continue an open/matched chat, but cannot
/// create the initial request. The caller has verified the MLS author and group.
pub(super) fn on_attachment(
    tx: &Transaction<'_>,
    owner: &str,
    group: &[u8],
    author: &str,
    id: [u8; 16],
) -> Result<bool, Error> {
    let peer = group_peer(tx, owner, group, author)?;
    let own = author == owner;
    let allowed = matched(tx, owner, &peer)?
        || match load(tx, &peer)? {
            Conversation::Open => true,
            Conversation::RequestSent(_) => !own,
            Conversation::RequestReceived(_) => own,
            Conversation::None | Conversation::Closed => false,
        };
    if !allowed {
        return Ok(false);
    }
    on_text(tx, owner, group, author, id, false)
}

/// Whether an edit or delete of `target` applies: while a request is pending only the pending
/// text may change, and nothing changes in a closed chat.
pub(super) fn allows_operation(
    tx: &Transaction<'_>,
    owner: &str,
    group: &[u8],
    author: &str,
    target: [u8; 16],
) -> Result<bool, Error> {
    let peer = group_peer(tx, owner, group, author)?;
    Ok(match load(tx, &peer)? {
        Conversation::RequestReceived(request) => author == owner || request == target,
        Conversation::Closed => false,
        _ => true,
    })
}

/// After a like or unlike changed the latest state, in its transaction: open the chat when a
/// match now exists, close it when `was_matched` ended, and then erase this device's copy if
/// `own` unlike ended it.
pub(super) fn on_like(
    tx: &Transaction<'_>,
    owner: &str,
    peer_id: &str,
    was_matched: bool,
    own: bool,
) -> Result<(), Error> {
    if matched(tx, owner, peer_id)? {
        save(tx, peer_id, Conversation::Open)?;
    } else if was_matched {
        save(tx, peer_id, Conversation::Closed)?;
        if own {
            super::messages::clear_pair_tx(tx, peer_id)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::messages::{MessageKey, VerifiedChatSender};
    use super::super::tests::{catalog_group, temp_db_path};
    use super::super::{Error, InboxKey, Store};
    use super::Conversation;
    use crate::chat::Body;
    use crate::interactions::{Interaction, Kind};
    use crate::prepare_put;
    use dyapp_identity::Identity;
    use dyapp_messaging::hlc::Timestamp;

    const GROUP: &[u8] = &[3];
    const DEVICE: [u8; 32] = [5; 32];

    struct Pair {
        store: Store,
        own: String,
        peer: String,
    }

    fn pair() -> Pair {
        let owner = Identity::generate();
        let peer = Identity::generate().peer_id();
        let mut store = Store::open(&temp_db_path(), &owner).unwrap();
        catalog_group(&mut store, GROUP, &peer);
        Pair {
            store,
            own: owner.peer_id(),
            peer,
        }
    }

    fn text(id: u8) -> Body {
        Body::Text {
            id: [id; 16],
            hlc: None,
            text: vec![id],
        }
    }

    impl Pair {
        fn send(&mut self, body: &Body) -> Result<(), Error> {
            let sender = VerifiedChatSender {
                group: GROUP,
                author: &self.own,
                device: &DEVICE,
            };
            let write = prepare_put(&[7; 32], vec![1]).unwrap();
            self.store
                .stage_chat_outbound(b"mls", &[write], sender, body, 1_000)
        }

        fn receive(&mut self, envelope: u8, body: &Body) -> bool {
            let sender = VerifiedChatSender {
                group: GROUP,
                author: &self.peer,
                device: &DEVICE,
            };
            let inbox = InboxKey {
                mailbox: [1; 32],
                id: [envelope; 16],
            };
            self.store
                .stage_chat_inbound(b"mls", &inbox, sender, body, 1_000)
                .unwrap()
        }

        fn shown(&self, author: &str, id: u8) -> Option<Vec<u8>> {
            let key = MessageKey {
                group: GROUP.to_vec(),
                author: author.to_owned(),
                id: [id; 16],
            };
            self.store.chat_message(&key).unwrap().and_then(|m| m.text)
        }

        fn state(&self) -> Conversation {
            self.store.conversation(&self.peer).unwrap()
        }

        fn own_like(&mut self, kind: Kind) {
            let peer = self.peer.clone();
            self.store.interact(&peer, kind, &[1], 1_000).unwrap();
        }

        fn peer_like(&mut self, kind: Kind, wall_ms: u64) {
            let event = Interaction {
                kind,
                event_id: [wall_ms.to_be_bytes()[6]; 16],
                hlc: Timestamp::new(wall_ms, 0).into(),
            };
            let (peer, own) = (self.peer.clone(), self.own.clone());
            assert!(self
                .store
                .apply_interaction(&peer, &own, &[2], event)
                .unwrap());
        }
    }

    #[test]
    fn own_request_waits_for_one_reply() {
        let mut p = pair();
        assert_eq!(p.state(), Conversation::None);
        p.send(&text(1)).unwrap();
        assert_eq!(p.state(), Conversation::RequestSent([1; 16]));
        assert!(matches!(p.send(&text(2)), Err(Error::RequestPending)));
        assert_eq!(p.shown(&p.own.clone(), 2), None);
        let (operation, hlc) = p.store.draft_chat_operation(2_000).unwrap();
        let edit = Body::Edit {
            target: [1; 16],
            operation,
            hlc,
            text: b"fixed".to_vec(),
        };
        p.send(&edit).unwrap();
        assert!(p.receive(1, &text(3)));
        assert_eq!(p.state(), Conversation::Open);
        p.send(&text(2)).unwrap();
    }

    #[test]
    fn second_peer_request_is_acked_without_history() {
        let mut p = pair();
        let peer = p.peer.clone();
        assert!(p.receive(1, &text(1)));
        assert_eq!(p.state(), Conversation::RequestReceived([1; 16]));
        assert!(!p.receive(2, &text(2)));
        assert_eq!(p.shown(&peer, 2), None);
        assert!(p.store.pending_acks().unwrap().contains(&InboxKey {
            mailbox: [1; 32],
            id: [2; 16],
        }));
        let edit = |target: u8, operation: u8| Body::Edit {
            target: [target; 16],
            operation: [operation; 16],
            hlc: Timestamp::new(100, 0).into(),
            text: b"edited".to_vec(),
        };
        assert!(p.receive(3, &edit(1, 8)));
        assert_eq!(p.shown(&peer, 1).as_deref(), Some(&b"edited"[..]));
        assert!(!p.receive(4, &edit(2, 9)));

        p.send(&text(5)).unwrap();
        assert_eq!(p.state(), Conversation::Open);
        assert!(p.receive(5, &text(6)));
        assert!(p.shown(&peer, 6).is_some());
    }

    #[test]
    fn open_survives_clear_and_unblock() {
        let mut p = pair();
        let peer = p.peer.clone();
        assert!(p.receive(1, &text(1)));
        p.own_like(Kind::Like);
        assert_eq!(p.state(), Conversation::RequestReceived([1; 16]));
        p.peer_like(Kind::Like, 500);
        assert_eq!(p.state(), Conversation::Open);

        p.store.clear_chat(GROUP).unwrap();
        p.store.set_blocked(&peer, true).unwrap();
        p.store.set_blocked(&peer, false).unwrap();
        assert_eq!(p.state(), Conversation::Open);
        assert!(p.receive(2, &text(2)));
        p.send(&text(3)).unwrap();
    }

    fn matched_pair() -> Pair {
        let mut p = pair();
        p.own_like(Kind::Like);
        p.peer_like(Kind::Like, 500);
        assert_eq!(p.state(), Conversation::Open);
        p
    }

    #[test]
    fn own_unlike_closes_the_match_and_erases_only_this_copy() {
        let mut p = matched_pair();
        let peer = p.peer.clone();
        assert!(p.receive(1, &text(1)));
        p.own_like(Kind::Unlike);
        assert_eq!(p.state(), Conversation::Closed);
        assert_eq!(p.shown(&peer, 1), None);
        assert!(p.store.chat_rows().unwrap().is_empty());

        assert!(!p.receive(2, &text(2)));
        assert_eq!(p.shown(&peer, 2), None);
        assert!(matches!(p.send(&text(3)), Err(Error::ChatClosed)));
        assert!(p.store.pending_acks().unwrap().contains(&InboxKey {
            mailbox: [1; 32],
            id: [2; 16],
        }));
    }

    #[test]
    fn peer_unlike_closes_the_match_and_keeps_history_until_a_new_match() {
        let mut p = matched_pair();
        let peer = p.peer.clone();
        assert!(p.receive(1, &text(1)));
        p.peer_like(Kind::Unlike, 600);
        assert_eq!(p.state(), Conversation::Closed);
        assert!(p.shown(&peer, 1).is_some());
        assert_eq!(p.store.chat_rows().unwrap().len(), 1);
        assert!(matches!(p.send(&text(2)), Err(Error::ChatClosed)));
        let edit = Body::Edit {
            target: [1; 16],
            operation: [8; 16],
            hlc: Timestamp::new(700, 0).into(),
            text: b"later".to_vec(),
        };
        assert!(!p.receive(2, &edit));
        assert_eq!(p.shown(&peer, 1).as_deref(), Some(&[1][..]));

        p.peer_like(Kind::Like, 800);
        assert_eq!(p.state(), Conversation::Open);
        assert!(p.receive(3, &text(3)));
        p.send(&text(4)).unwrap();
    }

    #[test]
    fn an_unlike_without_a_match_leaves_the_chat_open() {
        let mut p = pair();
        assert!(p.receive(1, &text(1)));
        p.send(&text(2)).unwrap();
        assert_eq!(p.state(), Conversation::Open);
        p.own_like(Kind::Like);
        p.own_like(Kind::Unlike);
        assert_eq!(p.state(), Conversation::Open);
        assert!(p.receive(3, &text(3)));
    }

    #[test]
    fn someone_i_unmatched_is_not_listed_in_liked_you() {
        let mut p = matched_pair();
        p.own_like(Kind::Unlike);
        assert!(p.store.liked_by().unwrap().is_empty());
    }

    #[test]
    fn a_text_outside_a_cataloged_group_is_refused() {
        let mut p = pair();
        let stranger = VerifiedChatSender {
            group: &[9],
            author: &p.peer.clone(),
            device: &DEVICE,
        };
        let inbox = InboxKey {
            mailbox: [1; 32],
            id: [1; 16],
        };
        assert!(matches!(
            p.store
                .stage_chat_inbound(b"mls", &inbox, stranger, &text(1), 1_000),
            Err(Error::GroupMismatch)
        ));
        assert_eq!(p.state(), Conversation::None);
    }
}
