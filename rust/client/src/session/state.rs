//! Session restoration and read-only state access.

use super::{
    CurrentMessage, Device, Error, Identity, MessageKey, Scores, Session, SignedRecord, Store,
};

impl Session {
    /// Open the last committed snapshot. The app supplies its protected sealing key.
    ///
    /// # Errors
    /// Returns [`Error::MissingSnapshot`] if no snapshot exists, [`Error::Store`]
    /// for database failures, or [`Error::Mls`] if restoring the sealed device fails.
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
            scores: Scores::default(),
        })
    }

    /// Open the last committed snapshot or, on the first open, certify a new device for
    /// `identity` with the mailbox key `device_key` and commit its snapshot first.
    ///
    /// # Errors
    /// Returns storage or MLS errors when creating, sealing, saving or restoring
    /// the device, and any restoration error from [`Self::open`].
    pub fn open_or_create(
        mut store: Store,
        identity: &Identity,
        device_key: &Identity,
        seal_key: &[u8; 32],
    ) -> Result<Self, Error> {
        if store.sealed_device()?.is_none() {
            let device = Device::new(identity, device_key.public_key(), 1)?;
            store.stage_outbound(&device.seal(seal_key)?, &[])?;
        }
        Self::open(store, seal_key)
    }

    /// The profile's store, for the calls outside chats such as the profile publish.
    #[must_use]
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The scores of the nodes this session used.
    #[must_use]
    pub fn scores(&self) -> &Scores {
        &self.scores
    }

    /// Read one current logical message, including its edited or deleted state.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if reading or decoding the stored message fails.
    pub fn chat_message(&self, key: &MessageKey) -> Result<Option<CurrentMessage>, Error> {
        Ok(self.store.chat_message(key)?)
    }

    /// The latest like state in both directions with `peer_id`.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if the stored like state cannot be read.
    pub fn likes(&self, peer_id: &str) -> Result<crate::store::Likes, Error> {
        Ok(self.store.likes(peer_id)?)
    }

    /// Where the chat with `peer_id` stands.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if the conversation state cannot be read.
    pub fn conversation(&self, peer_id: &str) -> Result<crate::store::Conversation, Error> {
        Ok(self.store.conversation(peer_id)?)
    }

    /// This device's signed certificate, for the owner's profile device list.
    ///
    /// # Errors
    /// Returns [`Error::Invalidated`] if the session device must be reopened.
    pub fn certificate(&self) -> Result<SignedRecord, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        Ok(device.certificate().clone())
    }
}
