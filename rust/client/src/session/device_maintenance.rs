//! Common after-mailbox membership maintenance for the owner's devices.
use super::{Error, Session};
use crate::Node;
use dyapp_profile::VerifiedProfile;

impl Session {
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
