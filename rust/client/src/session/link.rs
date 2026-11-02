//! Adds devices the owner linked to the groups this device holds and starts the private group of
//! the owner's devices ([ADR 0040](../../../../docs/decisions/0040-device-linking.md)).

use super::{prepare, Error, Session};
use crate::chat::{self, Body, Event};
use crate::{mailbox, scores, Node, PreparedEnvelope};
use dyapp_messaging::mls::Group;
use dyapp_p2p_net::proto::EnvelopeClass;
use dyapp_profile::VerifiedProfile;

/// How long each own device earlier in [`Group::add_turn`] gets to add a linked device first. Two
/// commits on one epoch fork a group for good, and no server orders them.
pub const ADD_TURN_SECONDS: u64 = 3600;

impl Session {
    /// The group changes reach the other own devices through. Two devices can start one each, so
    /// every device sends in the lowest ID and accepts from any.
    pub fn own_group(&self) -> Option<Vec<u8>> {
        self.own_groups().map(Group::id).min().map(<[u8]>::to_vec)
    }

    fn own_groups(&self) -> impl Iterator<Item = &Group> {
        let owner = self.store.owner_peer_id();
        self.groups.iter().filter(move |group| {
            group
                .members()
                .is_ok_and(|members| members.iter().all(|member| member == owner))
        })
    }

    pub(super) fn is_own_group(&self, id: &[u8]) -> bool {
        self.own_groups().any(|group| group.id() == id)
    }

    /// Whether `event` is anything but a receipt in an own group, which holds no conversation.
    pub(super) fn stray_in_own_group(&self, event: &Event) -> bool {
        match event {
            Event::Message {
                body: Body::Receipt(_),
                ..
            } => false,
            Event::Message { group, .. } | Event::Interaction { group, .. } => {
                self.is_own_group(group)
            }
            _ => false,
        }
    }

    /// Start the own group when the owner's newest profile `own` lists a second device, none
    /// exists here and this device's turn has come: the first by mailbox starts at once, the next
    /// after [`ADD_TURN_SECONDS`], so a device gone for good does not stop the others. Returns
    /// whether it started. Run it after the mailbox, so a Welcome from an earlier turn is seen
    /// first; [`Self::add_own_devices`] brings in later devices.
    pub async fn join_own_devices(
        &mut self,
        nodes: &mut [Node],
        own: &VerifiedProfile,
        now: u64,
        seal_key: &[u8; 32],
    ) -> Result<bool, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let listed = &own.profile.devices;
        if listed.len() < 2
            || own.peer_id != self.store.owner_peer_id()
            || !listed.contains(device.certificate())
            || self.own_group().is_some()
        {
            return Ok(false);
        }
        let key: [u8; 32] = device
            .device_key()?
            .try_into()
            .map_err(|_| crate::Error::Reply("bad device key"))?;
        let address = mailbox(&key);
        let mut turn = 0;
        for certificate in listed {
            if chat::device_mailbox(certificate)?.as_slice() < address.as_slice() {
                turn += 1;
            }
        }
        let age = self.store.own_profile_age(own.profile.version, now)?;
        if age < turn * ADD_TURN_SECONDS {
            return Ok(false);
        }
        let profiles = std::slice::from_ref(own);
        let packages = self.key_packages(nodes, profiles).await?;
        let (_, writes) = self.stage_new_group(own, &packages, profiles, seal_key)?;
        self.send_staged(nodes, writes, seal_key).await?;
        Ok(true)
    }

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
