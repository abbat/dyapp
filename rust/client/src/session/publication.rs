//! Ordinary field publication uses current verified directory authority, never an editable draft.
use super::{Error, Identity, Node, Session};
use crate::publish;
use crate::store::{ProfilePublication, ProfilePublicationOutcome};

impl Session {
    /// Publish a captured editable-field request using this original device.
    /// The caller applies the shared attestation/age/proof gates to the prepared request first.
    /// Only a genuinely new account with no prior directory can enroll its first device here.
    /// Pending authenticated links complete separately through original-session link recovery.
    ///
    /// # Errors
    /// Refuses wrong owner/device credentials, pending or uncertain enrollment, revoked/missing
    /// authorization and ordinary publication failures. Remote acceptance may precede local failure.
    pub async fn publish_profile(
        &self,
        node: &mut Node,
        identity: &Identity,
        device_key: &Identity,
        mut attempt: ProfilePublication,
    ) -> Result<ProfilePublicationOutcome, Error> {
        let certificate = self.certificate()?;
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        if self.store.owner_peer_id() != identity.peer_id()
            || certificate.public_key != identity.public_key()
            || device.device_key()? != device_key.public_key()
        {
            return Err(crate::store::Error::WrongIdentity.into());
        }
        let source = publish::prepare_fields(&self.store, node, identity, &mut attempt).await?;
        if let Some(source) = source {
            if self.own_device_revoked(&source)? {
                return Err(Error::Revoked);
            }
            if !source.devices.contains(&certificate) {
                return Err(publish::Failure::DirectoryAuthorityRequired.into());
            }
        } else {
            publish::list_device(
                attempt.draft_mut(),
                certificate,
                crate::owner_binding(device_key, &identity.public_key()),
            );
        }
        let outcome = publish::publish_prepared(&self.store, node, identity, &attempt).await?;
        self.store.note_device_listed(outcome.published.version)?;
        Ok(outcome)
    }
}
