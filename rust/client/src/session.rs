//! Durable outbound MLS session. A failed local write discards advanced in-memory state.

use crate::chat;
use crate::store::Store;
use crate::{prepare_put, Node, PreparedEnvelope};
use dyapp_messaging::mls::{Device, Group};

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
    groups: Vec<(Group, String)>,
}

impl Session {
    /// Open the last committed snapshot. The app supplies its protected sealing key.
    pub fn open(store: Store, seal_key: &[u8; 32]) -> Result<Self, Error> {
        let sealed = store.sealed_device()?.ok_or(Error::MissingSnapshot)?;
        let device = Device::open(&sealed, seal_key)?;
        let groups = store.load_groups(&device)?;
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
        self.groups = self.store.load_groups(&device)?;
        self.device = Some(device);
        Ok(())
    }

    /// Commit the new MLS state and exact outbound bytes before the first network request.
    fn stage_message(
        &mut self,
        group_id: &[u8],
        plaintext: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<Vec<PreparedEnvelope>, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let group = self
            .groups
            .iter_mut()
            .find(|(group, _)| group.id() == group_id)
            .ok_or(Error::UnknownGroup)?;
        let staged = (|| {
            let deliveries = chat::message(&mut group.0, device, plaintext)?;
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

    /// Send a message only after the new MLS state and every signed envelope are durable.
    pub async fn send_message(
        &mut self,
        node: &mut Node,
        group_id: &[u8],
        plaintext: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let writes = self.stage_message(group_id, plaintext, seal_key)?;
        for write in &writes {
            node.put_prepared(write).await?;
        }
        Ok(())
    }

    /// Reopen exact signed bytes for the retry worker after a crash or failed node request.
    pub fn due_outbound(&self, now_ms: i64) -> Result<Vec<PreparedEnvelope>, Error> {
        Ok(self.store.due_outbound(now_ms)?)
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, Session};
    use crate::store::Store;
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
            .stage_message(&group_id, b"hello", &seal_key)
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
            session.stage_message(&group_id, b"hello", &seal_key),
            Err(Error::Store(_))
        ));
        assert!(matches!(
            session.stage_message(&group_id, b"retry", &seal_key),
            Err(Error::Invalidated)
        ));
        assert!(session.due_outbound(0).unwrap().is_empty());
        assert_eq!(session.store.sealed_device().unwrap(), Some(sealed));
        session.recover(&seal_key).unwrap();
        assert!(session.device.is_some());
    }
}
