//! Validate a complete inventory before recording any advertised sources or gaps.
use super::{Blob, Repair, WINDOW};
use crate::replication::MediaManifestExt;
use dyapp_p2p_net::proto;
use libp2p::PeerId;
use std::collections::HashSet;

pub(super) fn valid(requested: &[proto::MediaPart], inventory: &proto::MediaInventory) -> bool {
    let mut checked = HashSet::new();
    for part in inventory.present.iter().chain(&inventory.missing) {
        if !requested.contains(part) || !checked.insert((part.hash.clone(), part.index)) {
            return false;
        }
    }
    inventory.whole.len() <= WINDOW
        && inventory.manifests.len() <= WINDOW
        && inventory
            .whole
            .iter()
            .all(|hash| requested.iter().any(|part| part.hash == *hash))
        && inventory.manifests.iter().all(|manifest| {
            manifest.data_shards().is_some()
                && requested.iter().any(|part| part.hash == manifest.hash)
        })
}

impl Repair {
    pub(super) fn inventory_response(
        &mut self,
        peer: PeerId,
        response: Option<proto::MediaResponse>,
    ) {
        let Some(inventory) = response.and_then(|response| response.inventory) else {
            self.unconfirmed = true;
            return;
        };
        self.inventory(peer, &inventory);
    }
}

impl Blob {
    pub(super) fn record_inventory(&mut self, peer: PeerId, inventory: &proto::MediaInventory) {
        if inventory.whole.contains(&self.hash) && !self.sources.contains(&peer) {
            self.sources.push(peer);
        }
        let manifest = inventory
            .manifests
            .iter()
            .find(|manifest| manifest.hash == self.hash);
        if self.manifest.is_none() {
            self.manifest = manifest.cloned();
        }
        for part in inventory
            .present
            .iter()
            .filter(|part| part.hash == self.hash)
        {
            let Some(index) = part.index else {
                continue;
            };
            if !manifest.is_some_and(|manifest| {
                Some(manifest) == self.manifest.as_ref()
                    && (index as usize) < manifest.shard_hashes.len()
            }) {
                continue;
            }
            let sources = self.shard_sources.entry(index).or_default();
            if !sources.contains(&peer) {
                sources.push(peer);
            }
        }
        for part in inventory
            .missing
            .iter()
            .filter(|part| part.hash == self.hash)
        {
            if !self.gaps.contains(&(peer, part.index)) {
                self.gaps.push((peer, part.index));
            }
        }
    }
}
