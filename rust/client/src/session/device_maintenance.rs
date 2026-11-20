//! Common after-mailbox membership maintenance for the owner's devices.
use super::{Error, Session};
use crate::{profiles::Profiles, Node};
use dyapp_profile::VerifiedProfile;

#[cfg(test)]
mod tests;

impl Session {
    /// Admit verified remote owner authority without replacing the editable profile draft.
    ///
    /// Held equal/newer authority and final account tombstones follow [`Profiles::accept`].
    /// Ordinary version clocks survive cache/session reopen. Revocation is remembered before
    /// it is reported, so a later older read cannot restore the dropped certificate.
    ///
    /// # Errors
    /// Returns wrong-owner, invalid authority, durable rollback, revocation or storage failures.
    pub fn admit_own_profile(
        &self,
        known: &mut Profiles,
        profile: VerifiedProfile,
        now: u64,
    ) -> Result<VerifiedProfile, Error> {
        let owner = self.store.owner_peer_id();
        if profile.peer_id != owner {
            return Err(crate::store::Error::WrongIdentity.into());
        }
        // Select only this owner's candidate before touching the shared cache or durable clocks.
        let mut selected = Profiles::default();
        if let Some(held) = known.get(owner) {
            selected.accept(held.clone())?;
        }
        match selected.accept(profile) {
            Ok(_) | Err(crate::Error::Rollback | crate::Error::Deleted) => {}
            Err(error) => return Err(error.into()),
        }
        let admitted = selected.get(owner).cloned().ok_or(crate::Error::Rollback)?;
        if !admitted.profile.deleted
            && admitted.profile.version < self.store.ordinary_profile_floor()?
        {
            return Err(crate::Error::Rollback.into());
        }
        self.store.own_profile_age(admitted.profile.version, now)?;
        known.accept(admitted.clone())?;
        if self.own_device_revoked(&admitted.profile)? {
            return Err(Error::Revoked);
        }
        Ok(admitted)
    }

    /// Start the private own group, add listed devices, then remove dropped devices.
    ///
    /// Call after applying the mailbox, using verified current owner/profile authority. The
    /// caller retains its rollback, network-generation and revocation admission boundaries.
    /// Every stage is attempted before the first failure is returned, preserving desktop's
    /// best-effort maintenance ordering. This never edits or republishes the public draft.
    ///
    /// # Errors
    /// Returns the first group creation, membership, storage or network failure after all
    /// stages have been attempted. Earlier local or network steps may already be committed.
    pub async fn maintain_own_devices(
        &mut self,
        nodes: &mut [Node],
        own: &VerifiedProfile,
        profiles: &[VerifiedProfile],
        now: u64,
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let joined = self.join_own_devices(nodes, own, now, seal_key).await;
        let added = self
            .add_own_devices(nodes, own, profiles, now, seal_key)
            .await;
        let removed = self
            .remove_dropped_devices(nodes, profiles, now, seal_key)
            .await;
        joined?;
        added?;
        removed?;
        Ok(())
    }
}
