//! Adds devices the owner linked to the groups this device holds
//! ([ADR 0040](../../../../docs/decisions/0040-device-linking.md)).

use super::{prepare, Error, Session};
use crate::{chat, mailbox, scores, Node, PreparedEnvelope};
use dyapp_messaging::mls::Group;
use dyapp_p2p_net::proto::EnvelopeClass;
use dyapp_profile::VerifiedProfile;

/// How long each own device earlier in [`Group::add_turn`] gets to add a linked device first. Two
/// commits on one epoch fork a group for good, and no server orders them.
pub const ADD_TURN_SECONDS: u64 = 3600;

impl Session {
    /// Add the devices the owner's newest profile `own` lists to every group here that lacks them
    /// and whose turn has come; returns how many groups were committed to. A device with no
    /// KeyPackage yet joins on a later call. Run it after the mailbox, so a commit from an earlier
    /// turn is seen first.
    pub async fn add_own_devices(
        &mut self,
        nodes: &mut [Node],
        own: &VerifiedProfile,
        profiles: &[VerifiedProfile],
        now: u64,
        seal_key: &[u8; 32],
    ) -> Result<usize, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let age = self.store.own_profile_age(own.profile.version, now)?;
        let mut due = Vec::new();
        for group in &self.groups {
            let missing = group.missing_devices(own)?;
            let wait = group
                .add_turn(device, own)?
                .saturating_mul(ADD_TURN_SECONDS);
            if !missing.is_empty() && age >= wait {
                due.push((group.id().to_vec(), missing));
            }
        }
        let mut added = 0;
        for (group, missing) in due {
            let mut packages = Vec::new();
            for key in missing {
                let key: [u8; 32] = key
                    .try_into()
                    .map_err(|_| crate::Error::Reply("bad device key"))?;
                let address = mailbox(&key).to_vec();
                let fetched = scores::first(&mut self.scores, nodes, Node::peer, async |node| {
                    node.fetch_key_package(own, &address).await
                })
                .await;
                if let Ok(package) = fetched {
                    packages.push((address, package));
                }
            }
            if !packages.is_empty() {
                let writes = self.stage_add(&group, &packages, profiles, seal_key)?;
                self.send_staged(nodes, writes, seal_key).await?;
                added += 1;
            }
        }
        Ok(added)
    }

    /// Commit the new MLS state and the exact commit and Welcome writes before the first request.
    fn stage_add(
        &mut self,
        group_id: &[u8],
        packages: &[(Vec<u8>, Vec<u8>)],
        profiles: &[VerifiedProfile],
        seal_key: &[u8; 32],
    ) -> Result<Vec<PreparedEnvelope>, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let group = self
            .groups
            .iter_mut()
            .find(|group| group.id() == group_id)
            .ok_or(Error::UnknownGroup)?;
        let staged = (|| {
            let deliveries = chat::add(group, device, packages, profiles)?;
            let writes = prepare(&self.store, deliveries, EnvelopeClass::Message, seal_key)?;
            self.store
                .stage_outbound(&device.seal(seal_key)?, &writes)?;
            Ok(writes)
        })();
        if staged.is_err() {
            self.device = None;
            self.groups.clear();
        }
        staged
    }
}
