//! Adds devices the owner linked to the groups this device holds and starts the private group of
//! the owner's devices ([ADR 0040](../../../../docs/decisions/0040-device-linking.md)).

use super::{prepare, Error, Session};
use crate::chat::{self, Body, Delivery, Event, PeerChange};
use crate::{mailbox, scores, Node, PreparedEnvelope};
use dyapp_identity::Identity;
use dyapp_messaging::mls::{Device, Group};
use dyapp_p2p_net::proto::EnvelopeClass;
use dyapp_profile::VerifiedProfile;

/// How long each device earlier in [`Group::add_turn`] or [`Group::remove_turn`] gets to commit
/// first. Two commits on one epoch fork a group for good, and no server orders them.
pub const ADD_TURN_SECONDS: u64 = 3600;

impl Session {
    /// The lowest ID of the groups changes reach the other own devices through. Two devices can
    /// start one each, so every device sends in all of them and accepts from any.
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

    /// Make `change` about `peer` here and tell the other own devices, in the one commit that
    /// stages the send. With no own group it stays here. A write no node takes now waits in the
    /// outbox for [`Self::retry_due`].
    pub async fn change_peer(
        &mut self,
        nodes: &mut [Node],
        peer: &str,
        change: PeerChange,
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        if !change.valid(peer) {
            return Err(crate::store::Error::InvalidName.into());
        }
        let hlc = self.store.tick(crate::now() * 1000)?;
        let groups: Vec<_> = self.own_groups().map(|g| g.id().to_vec()).collect();
        if groups.is_empty() {
            self.store.apply_peer_change(peer, &change, hlc)?;
            return Ok(());
        }
        let body = Body::PeerChange {
            peer: peer.to_owned(),
            hlc: hlc.into(),
            change,
        };
        for group in groups {
            match self.send_body(nodes, &group, &body, seal_key).await {
                Err(Error::Client(_)) | Ok(()) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Sign `contact` no more tokens and revoke those it holds through the first node that
    /// answers.
    pub(super) async fn revoke_tokens(
        &mut self,
        nodes: &mut [Node],
        device_key: &Identity,
        contact: &str,
    ) -> Result<(), Error> {
        let keys = self
            .store
            .revoke_issued(contact, (crate::now() / 86_400) as u32)?;
        if keys.is_empty() {
            return Ok(());
        }
        scores::first(&mut self.scores, nodes, Node::peer, async |node| {
            node.revoke_tokens(device_key, &keys).await
        })
        .await?;
        Ok(())
    }

    /// [`Self::revoke_tokens`] for a block another own device made: each device issued its own
    /// tokens. With no node to tell, the tokens stay marked revoked and the rest of the mailbox
    /// page goes on.
    pub(super) async fn revoke_after_own_block(
        &mut self,
        nodes: &mut [Node],
        device_key: &Identity,
        contact: &str,
    ) -> Result<(), Error> {
        match self.revoke_tokens(nodes, device_key, contact).await {
            Err(Error::Client(_)) | Ok(()) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Whether `event` does not belong where it arrived: an own change outside an own group, or
    /// anything but a receipt or an own change in one, which holds no conversation.
    pub(super) fn misplaced(&self, event: &Event) -> bool {
        match event {
            Event::Message {
                group,
                body: Body::PeerChange { .. },
                ..
            } => !self.is_own_group(group),
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

    /// Remove the devices `profiles` no longer lists from every group here whose turn has come;
    /// returns how many groups were committed to. Run it after the mailbox, so a commit from an
    /// earlier turn is seen first.
    pub async fn remove_dropped_devices(
        &mut self,
        nodes: &mut [Node],
        profiles: &[VerifiedProfile],
        now: u64,
        seal_key: &[u8; 32],
    ) -> Result<usize, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let mut due = Vec::new();
        for group in &self.groups {
            let Some(turn) = group.remove_turn(device, profiles)? else {
                self.store.forget_dropped_leaf(group.id())?;
                continue;
            };
            let wait = turn.saturating_mul(ADD_TURN_SECONDS);
            if self.store.dropped_leaf_age(group.id(), now)? >= wait {
                due.push(group.id().to_vec());
            }
        }
        for group in &due {
            let remove = |group: &mut Group, device: &Device| chat::remove(group, device, profiles);
            let writes = self.stage_commit(group, seal_key, remove)?;
            self.send_staged(nodes, writes, seal_key).await?;
        }
        Ok(due.len())
    }

    /// Commit the new MLS state and the exact commit and Welcome writes before the first request.
    fn stage_add(
        &mut self,
        group_id: &[u8],
        packages: &[(Vec<u8>, Vec<u8>)],
        profiles: &[VerifiedProfile],
        seal_key: &[u8; 32],
    ) -> Result<Vec<PreparedEnvelope>, Error> {
        self.stage_commit(group_id, seal_key, |group, device| {
            chat::add(group, device, packages, profiles)
        })
    }

    fn stage_commit(
        &mut self,
        group_id: &[u8],
        seal_key: &[u8; 32],
        commit: impl FnOnce(&mut Group, &Device) -> Result<Vec<Delivery>, crate::Error>,
    ) -> Result<Vec<PreparedEnvelope>, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let group = self
            .groups
            .iter_mut()
            .find(|group| group.id() == group_id)
            .ok_or(Error::UnknownGroup)?;
        let staged = (|| {
            let deliveries = commit(group, device)?;
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
