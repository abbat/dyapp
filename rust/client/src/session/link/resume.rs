//! Resume an admitted existing device without bootstrapping a replacement session.
use super::{publish, Device, Error, Identity, Node, Session};

impl Session {
    /// Finish a pending link using this existing session and its original protected keys.
    /// Consumes the session, retaining it through actual completion. No device or sealing key is
    /// created. Exact replay or explicit owner confirmation must admit existing editable values.
    /// Returns false when no link is pending; the pending row is cleared only after directory,
    /// single-use packages and fallback completion all succeed.
    ///
    /// # Errors
    /// Refuses a wrong owner/device/sealing key before link-state import. Otherwise returns the
    /// same review-required, revoked, storage and network failures as [`Self::finish_link`].
    /// Earlier network/local steps may succeed before a failure; reopen the original session to retry.
    pub async fn resume_link(
        mut self,
        node: &mut Node,
        identity: &Identity,
        device_key: &Identity,
        key_packages: usize,
        seal_key: &[u8; 32],
        now: u64,
    ) -> Result<bool, Error> {
        self.admit_link_resume(identity, device_key, seal_key)?;
        let Some(state) = self.store.link_pending()? else {
            return Ok(false);
        };
        let adopted = publish::adopt(&mut self.store, node, identity, &state, now).await?;
        self.complete_link_directory(node, identity, device_key, &adopted.published, &state)
            .await?;
        self.complete_link_packages(node, device_key, key_packages, seal_key, now, &state)
            .await?;
        Ok(true)
    }

    fn admit_link_resume(
        &self,
        identity: &Identity,
        device_key: &Identity,
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        if self.store.owner_peer_id() != identity.peer_id()
            || device.certificate().public_key != identity.public_key()
            || device.device_key()? != device_key.public_key()
        {
            return Err(crate::store::Error::WrongIdentity.into());
        }
        let sealed = self.store.sealed_device()?.ok_or(Error::MissingSnapshot)?;
        let saved = Device::open(&sealed, seal_key)?;
        if saved.certificate() != device.certificate() {
            return Err(Error::Invalidated);
        }
        Ok(())
    }
}
