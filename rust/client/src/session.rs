//! Durable MLS session. A failed local write discards advanced in-memory state.

use crate::chat::{self, Body, Event};
use crate::store::{InboxKey, InboxKind, Store};
use crate::{prepare_put, Node, PreparedEnvelope};
use dyapp_identity::Identity;
use dyapp_messaging::mls::{Device, Group};
use dyapp_profile::VerifiedProfile;

/// A received message as kept in the inbox for the app's history; protobuf, so fields can be
/// added without a new format.
#[derive(Clone, PartialEq, prost::Message)]
pub struct StoredMessage {
    /// The group ID.
    #[prost(bytes = "vec", tag = "1")]
    pub group: Vec<u8>,
    /// Peer ID of the sender's identity.
    #[prost(string, tag = "2")]
    pub sender: String,
    /// The tagged body, [`chat::Body::encode`].
    #[prost(bytes = "vec", tag = "3")]
    pub body: Vec<u8>,
}

/// Why an outbound session cannot send.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No MLS snapshot has been committed for this profile.
    #[error("no saved MLS session")]
    MissingSnapshot,
    /// A local failure invalidated the advanced MLS state; reopen the saved snapshot.
    #[error("MLS session must be reopened")]
    Invalidated,
    /// The requested conversation is absent from the catalog.
    #[error("unknown MLS conversation")]
    UnknownGroup,
    /// The sealed MLS state could not be opened.
    #[error(transparent)]
    Mls(#[from] dyapp_messaging::mls::Error),
    /// Chat preparation or network delivery failed.
    #[error(transparent)]
    Client(#[from] crate::Error),
    /// The local database failed.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
}

/// One profile's restored MLS device, group handles and durable mailbox queue.
pub struct Session {
    store: Store,
    device: Option<Device>,
    groups: Vec<Group>,
}

impl Session {
    /// Open the last committed snapshot. The app supplies its protected sealing key.
    pub fn open(store: Store, seal_key: &[u8; 32]) -> Result<Self, Error> {
        let sealed = store.sealed_device()?.ok_or(Error::MissingSnapshot)?;
        let device = Device::open(&sealed, seal_key)?;
        let groups = store
            .load_groups(&device)?
            .into_iter()
            .map(|(g, _)| g)
            .collect();
        Ok(Self {
            store,
            device: Some(device),
            groups,
        })
    }

    /// Reopen the committed snapshot after a local failure; never reuse the advanced handles.
    pub fn recover(&mut self, seal_key: &[u8; 32]) -> Result<(), Error> {
        self.device = None;
        self.groups.clear();
        let sealed = self.store.sealed_device()?.ok_or(Error::MissingSnapshot)?;
        let device = Device::open(&sealed, seal_key)?;
        self.groups = self
            .store
            .load_groups(&device)?
            .into_iter()
            .map(|(g, _)| g)
            .collect();
        self.device = Some(device);
        Ok(())
    }

    /// Commit the new MLS state and exact outbound bytes before the first network request.
    fn stage_message(
        &mut self,
        group_id: &[u8],
        body: &Body,
        seal_key: &[u8; 32],
    ) -> Result<Vec<PreparedEnvelope>, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let group = self
            .groups
            .iter_mut()
            .find(|group| group.id() == group_id)
            .ok_or(Error::UnknownGroup)?;
        let staged = (|| {
            let deliveries = chat::message(group, device, body)?;
            let writes = deliveries
                .into_iter()
                .map(|delivery| prepare_put(&delivery.mailbox, delivery.mls))
                .collect::<Result<Vec<_>, _>>()?;
            let sealed = device.seal(seal_key)?;
            self.store.stage_outbound(&sealed, &writes)?;
            Ok(writes)
        })();
        if staged.is_err() {
            self.device = None;
            self.groups.clear();
        }
        staged
    }

    /// Send a text only after the new MLS state and every signed envelope are durable.
    pub async fn send_message(
        &mut self,
        node: &mut Node,
        group_id: &[u8],
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let writes = self.stage_message(group_id, &Body::Text(text.to_vec()), seal_key)?;
        for write in &writes {
            node.put_prepared(write).await?;
        }
        Ok(())
    }

    /// Reopen exact signed bytes for the retry worker after a crash or failed node request.
    pub fn due_outbound(&self, now_ms: i64) -> Result<Vec<PreparedEnvelope>, Error> {
        Ok(self.store.due_outbound(now_ms)?)
    }

    /// Fetch the mailbox of `device_key`, commit each new envelope before returning it, then
    /// acknowledge every committed ID, earlier ones whose ACK was lost included. On an error,
    /// events committed so far are in the inbox history and their ACKs stay pending.
    pub async fn receive(
        &mut self,
        node: &mut Node,
        device_key: &Identity,
        profiles: &[VerifiedProfile],
        seal_key: &[u8; 32],
    ) -> Result<Vec<Event>, Error> {
        let (envelopes, _more) = node.fetch(device_key).await?;
        let mailbox = crate::mailbox(&device_key.public_key());
        let mut events = Vec::new();
        let mut seen = Vec::new();
        for envelope in envelopes {
            // Nodes only store 16-byte IDs; leave anything else alone.
            let Ok(id) = envelope.id.as_slice().try_into() else {
                continue;
            };
            let key = InboxKey { mailbox, id };
            if self.store.has_inbox(&key)? {
                // Committed before: only the ACK can be missing, maybe lost after mark_acked.
                seen.push(key);
                continue;
            }
            events.extend(self.stage_envelope(&key, &envelope.ciphertext, profiles, seal_key)?);
        }
        let acks = self.acks(mailbox, seen)?;
        if !acks.is_empty() {
            node.ack(device_key, acks.iter().map(|key| key.id.to_vec()).collect())
                .await?;
            for key in &acks {
                self.store.mark_acked(key)?;
            }
        }
        Ok(events)
    }

    /// IDs to acknowledge at `mailbox`: the `seen` duplicates and every pending ACK there.
    fn acks(&self, mailbox: [u8; 32], mut seen: Vec<InboxKey>) -> Result<Vec<InboxKey>, Error> {
        for key in self.store.pending_acks()? {
            if key.mailbox == mailbox && !seen.contains(&key) {
                seen.push(key);
            }
        }
        Ok(seen)
    }

    /// Apply one new envelope and commit the result with its pending ACK. A Gap, an unknown
    /// owner or group is neither recorded nor acknowledged, so it is fetched again.
    fn stage_envelope(
        &mut self,
        key: &InboxKey,
        bytes: &[u8],
        profiles: &[VerifiedProfile],
        seal_key: &[u8; 32],
    ) -> Result<Option<Event>, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let mut event = match chat::apply(device, &mut self.groups, bytes, profiles) {
            Ok(None) => return Ok(None),
            Ok(Some(event @ (Event::Gap(_) | Event::UnknownOwner(_)))) => return Ok(Some(event)),
            Ok(Some(event)) => event,
            Err(error) => Event::Refused(error.to_string()),
        };
        let mut peer = None;
        if let Event::Joined(id) = &event {
            peer = self.partner(id);
            if peer.is_none() {
                event = Event::Refused("not a one-to-one conversation".into());
            }
        }
        let staged = if matches!(event, Event::Refused(_)) {
            // Keep the saved MLS state and reopen it: a refused envelope must change nothing.
            (|| {
                let sealed = self.store.sealed_device()?.ok_or(Error::MissingSnapshot)?;
                self.store
                    .stage_inbound(&sealed, key, InboxKind::Refused, b"", None)?;
                self.recover(seal_key)
            })()
        } else {
            (|| {
                let (kind, payload) = match &event {
                    Event::Message {
                        group,
                        sender,
                        body,
                    } => (
                        InboxKind::Message,
                        prost::Message::encode_to_vec(&StoredMessage {
                            group: group.clone(),
                            sender: sender.clone(),
                            body: body.encode(),
                        }),
                    ),
                    _ => (InboxKind::StateChange, Vec::new()),
                };
                let joined = match (&event, &peer) {
                    (Event::Joined(id), Some(peer)) => Some((id.as_slice(), peer.as_str())),
                    _ => None,
                };
                let sealed = device.seal(seal_key)?;
                self.store
                    .stage_inbound(&sealed, key, kind, &payload, joined)?;
                Ok(())
            })()
        };
        if staged.is_err() {
            self.device = None;
            self.groups.clear();
        }
        staged.map(|()| Some(event))
    }

    /// The one other owner in a joined group.
    // shortcut: a group of only this owner's devices or of three owners is refused; revisit
    // when multi-device sync or group chats exist.
    fn partner(&self, id: &[u8]) -> Option<String> {
        let group = self.groups.iter().find(|group| group.id() == id)?;
        let mut others = group.members().ok()?;
        others.retain(|member| member != self.store.owner_peer_id());
        others.sort();
        others.dedup();
        match <[String; 1]>::try_from(others) {
            Ok([peer]) => Some(peer),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, Session, StoredMessage};
    use crate::chat::{self, Body, Event};
    use crate::store::{InboxKey, InboxKind, Store};
    use dyapp_identity::Identity;
    use dyapp_messaging::mls::Device;
    use dyapp_profile::Profile;

    fn person() -> (Identity, Device, dyapp_profile::VerifiedProfile) {
        let identity = Identity::generate();
        let device = Device::new(&identity, Identity::generate().public_key(), 1).unwrap();
        let profile = Profile {
            version: 1,
            devices: vec![device.certificate().clone()],
            ..Default::default()
        }
        .sign(&identity);
        (identity, device, dyapp_profile::verify(&profile).unwrap())
    }

    #[test]
    fn message_is_staged_before_network_and_reopens_exact_bytes() {
        let (alice_id, alice, alice_profile) = person();
        let (_, bob, bob_profile) = person();
        let (group, _) = alice
            .create_group(
                &[bob.key_package(false).unwrap()],
                &[alice_profile, bob_profile.clone()],
            )
            .unwrap();
        let group_id = group.id().to_vec();
        let seal_key = [3; 32];
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "dyapp-send-session-{:032x}.db",
            u128::from_be_bytes(random)
        ));
        let mut store = Store::open(&path, &alice_id).unwrap();
        store
            .stage_group(&group, &bob_profile, &alice.seal(&seal_key).unwrap(), &[])
            .unwrap();
        let mut session = Session::open(store, &seal_key).unwrap();
        let writes = session
            .stage_message(&group_id, &Body::Text(b"hello".to_vec()), &seal_key)
            .unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(
            session.due_outbound(0).unwrap()[0].as_bytes(),
            writes[0].as_bytes()
        );
        drop(session);
        let session = Session::open(Store::open(&path, &alice_id).unwrap(), &seal_key).unwrap();
        assert_eq!(
            session.due_outbound(0).unwrap()[0].as_bytes(),
            writes[0].as_bytes()
        );
    }

    fn temp_path() -> std::path::PathBuf {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        std::env::temp_dir().join(format!(
            "dyapp-receive-session-{:032x}.db",
            u128::from_be_bytes(random)
        ))
    }

    fn key(id: u8) -> InboxKey {
        InboxKey {
            mailbox: [9; 32],
            id: [id; 16],
        }
    }

    /// Bob's session over a fresh store, invited by Alice to `group` with `welcome`.
    struct Invite {
        session: Session,
        bob_id: Identity,
        alice_id: Identity,
        alice: Device,
        group: dyapp_messaging::mls::Group,
        welcome: Vec<u8>,
        profiles: [dyapp_profile::VerifiedProfile; 2],
    }

    fn bob_invited(path: &std::path::Path, seal_key: &[u8; 32]) -> Invite {
        let (alice_id, alice, alice_profile) = person();
        let (bob_id, bob, bob_profile) = person();
        let profiles = [alice_profile, bob_profile];
        let (group, welcome) = alice
            .create_group(&[bob.key_package(false).unwrap()], &profiles)
            .unwrap();
        let mut store = Store::open(path, &bob_id).unwrap();
        store
            .stage_outbound(&bob.seal(seal_key).unwrap(), &[])
            .unwrap();
        Invite {
            session: Session::open(store, seal_key).unwrap(),
            bob_id,
            alice_id,
            alice,
            group,
            welcome,
            profiles,
        }
    }

    #[test]
    fn received_envelopes_commit_before_ack_and_survive_restart() {
        let path = temp_path();
        let seal_key = [5; 32];
        let Invite {
            mut session,
            bob_id,
            alice_id,
            alice,
            mut group,
            welcome,
            profiles,
        } = bob_invited(&path, &seal_key);
        assert!(
            session
                .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
                .unwrap()
                == Some(Event::Joined(group.id().to_vec()))
        );
        assert_eq!(
            session.store.known_groups().unwrap(),
            [(group.id().to_vec(), alice_id.peer_id())]
        );
        let hi = chat::message(&mut group, &alice, &Body::Text(b"hi".to_vec())).unwrap();
        assert!(matches!(
            session.stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key),
            Ok(Some(Event::Message { .. }))
        ));
        let payload: Vec<u8> = rusqlite::Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT `payload` FROM `inbox` WHERE `envelope_id` = ?1",
                [[2_u8; 16].as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            <StoredMessage as prost::Message>::decode(payload.as_slice()).unwrap()
                == StoredMessage {
                    group: group.id().to_vec(),
                    sender: alice_id.peer_id(),
                    body: Body::Text(b"hi".to_vec()).encode(),
                }
        );
        drop(session);

        // Restarted before the ACK: the joined group and both pending ACKs are back.
        let mut session = Session::open(Store::open(&path, &bob_id).unwrap(), &seal_key).unwrap();
        assert!(session.store.has_inbox(&key(2)).unwrap());
        assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1), key(2)]);
        let again = chat::message(&mut group, &alice, &Body::Text(b"again".to_vec())).unwrap();
        let Some(Event::Message { body, .. }) = session
            .stage_envelope(&key(3), &again[0].mls, &profiles, &seal_key)
            .unwrap()
        else {
            panic!("expected the next message");
        };
        assert!(body == Body::Text(b"again".to_vec()));

        // A duplicate seen after its ACK is acknowledged again; another mailbox is not.
        for id in [1, 2, 3] {
            assert!(session.store.mark_acked(&key(id)).unwrap());
        }
        let other = InboxKey {
            mailbox: [8; 32],
            id: [4; 16],
        };
        session
            .store
            .stage_inbound(b"sealed", &other, InboxKind::StateChange, b"", None)
            .unwrap();
        assert_eq!(session.acks([9; 32], vec![key(2)]).unwrap(), [key(2)]);
    }

    #[test]
    fn failed_receive_commit_neither_acks_nor_loses_the_envelope() {
        let path = temp_path();
        let seal_key = [6; 32];
        let Invite {
            mut session,
            alice,
            mut group,
            welcome,
            profiles,
            ..
        } = bob_invited(&path, &seal_key);
        session
            .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
            .unwrap();
        let hi = chat::message(&mut group, &alice, &Body::Text(b"hi".to_vec())).unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TRIGGER `fail_mls` BEFORE UPDATE ON `mls_state`
             BEGIN SELECT RAISE(ABORT, 'test failure'); END",
        )
        .unwrap();
        assert!(matches!(
            session.stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key),
            Err(Error::Store(_))
        ));
        assert!(!session.store.has_inbox(&key(2)).unwrap());
        assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1)]);
        assert!(matches!(
            session.stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key),
            Err(Error::Invalidated)
        ));

        db.execute_batch("DROP TRIGGER `fail_mls`").unwrap();
        session.recover(&seal_key).unwrap();
        let Some(Event::Message { body, .. }) = session
            .stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key)
            .unwrap()
        else {
            panic!("expected the message after recovery");
        };
        assert!(body == Body::Text(b"hi".to_vec()));
    }

    #[test]
    fn refused_envelope_is_marked_without_changing_mls_state() {
        let path = temp_path();
        let seal_key = [7; 32];
        let Invite {
            mut session,
            profiles,
            ..
        } = bob_invited(&path, &seal_key);
        let sealed = session.store.sealed_device().unwrap();
        assert!(matches!(
            session.stage_envelope(&key(1), b"not mls", &profiles, &seal_key),
            Ok(Some(Event::Refused(_)))
        ));
        assert!(session.store.has_inbox(&key(1)).unwrap());
        assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1)]);
        assert_eq!(session.store.sealed_device().unwrap(), sealed);
        assert!(session.device.is_some());
    }

    #[test]
    fn failed_store_write_invalidates_session_until_recovery() {
        let (alice_id, alice, alice_profile) = person();
        let (_, bob, bob_profile) = person();
        let (group, _) = alice
            .create_group(
                &[bob.key_package(false).unwrap()],
                &[alice_profile, bob_profile.clone()],
            )
            .unwrap();
        let group_id = group.id().to_vec();
        let seal_key = [4; 32];
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "dyapp-send-session-{:032x}.db",
            u128::from_be_bytes(random)
        ));
        let mut store = Store::open(&path, &alice_id).unwrap();
        let sealed = alice.seal(&seal_key).unwrap();
        store
            .stage_group(&group, &bob_profile, &sealed, &[])
            .unwrap();
        let mut session = Session::open(store, &seal_key).unwrap();
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER `fail_mls` BEFORE UPDATE ON `mls_state`
             BEGIN SELECT RAISE(ABORT, 'test failure'); END",
            )
            .unwrap();
        assert!(matches!(
            session.stage_message(&group_id, &Body::Text(b"hello".to_vec()), &seal_key),
            Err(Error::Store(_))
        ));
        assert!(matches!(
            session.stage_message(&group_id, &Body::Text(b"retry".to_vec()), &seal_key),
            Err(Error::Invalidated)
        ));
        assert!(session.due_outbound(0).unwrap().is_empty());
        assert_eq!(session.store.sealed_device().unwrap(), Some(sealed));
        session.recover(&seal_key).unwrap();
        assert!(session.device.is_some());
    }
}
