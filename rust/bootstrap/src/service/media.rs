//! The media role's requests and replica puts.

use super::*;

impl Service {
    /// Errors as in [`Service::profile`].
    pub fn media(&self, peer: &Peer, request: MediaRequest) -> crate::Result<MediaResponse> {
        if let Some(media_request::Request::Put(put)) = &request.request {
            return match self.prepare_media(peer, put)? {
                Ok(replica) => self.store_media(&replica),
                Err(status) => Ok(media_upload_status(status)),
            };
        }
        let Some(media) = &self.media else {
            return Ok(media_status(Status::Unsupported));
        };
        let is_replica = matches!(
            request.request,
            Some(media_request::Request::ReplicaPut(_) | media_request::Request::ShardPut(_))
        );
        if is_replica && !self.node_peer(peer) {
            self.strike(peer);
            return Ok(media_status(Status::Denied));
        }
        // The media limit is no strike: a client loading a gallery is not misbehaving.
        if self.shed(request.encoded_len(), SHED_MEDIA)
            || !if is_replica {
                self.admit_quiet(peer, 1)
            } else {
                self.admit(peer)
            }
            || !self.media_peers.check_limit(&peer.id)
        {
            return Ok(media_status(Status::RateLimited));
        }
        if self.refused(peer) {
            return Ok(media_status(Status::Refused));
        }
        let l = &self.config.limits;
        Ok(match request.request {
            Some(media_request::Request::Keep(record)) => {
                if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
                    return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
                }
                let Ok((owner, keep)) =
                    owner_request::<proto::MediaKeep>(&record, Domain::MediaKeep, |_| true)
                else {
                    self.strike(peer);
                    return Ok(media_status(Status::Denied));
                };
                if self.listed(&owner) {
                    return Ok(media_status(Status::Refused));
                }
                let now = chrono::Utc::now().timestamp().unsigned_abs();
                if keep.version == 0
                    || keep.hashes.len() > MAX_KEEP
                    || keep.hashes.iter().any(|h| h.len() != 32)
                {
                    return Ok(media_status(Status::Invalid));
                }
                if keep.time.abs_diff(now) > CLOCK_SKEW {
                    return Ok(media_clock_invalid(now));
                }
                match media.keep_signed(&hex(&owner), &keep, &record)? {
                    Some(missing) => MediaResponse {
                        missing,
                        ..media_status(Status::Ok)
                    },
                    None => media_status(Status::Stale),
                }
            }
            Some(media_request::Request::Put(_)) => unreachable!("put admitted separately"),
            // ponytail: a get names no owner, so a listed owner's blobs already stored are still
            // served unless the deny list names the blob hash too.
            Some(media_request::Request::Get(get)) => {
                if get.hash.len() != 32 {
                    return Ok(media_status(Status::Invalid));
                }
                if self.listed(&get.hash) {
                    return Ok(media_status(Status::Refused));
                }
                match media.get(&get.hash)? {
                    Some(data) => {
                        if !self.media_charge(peer, data.len() as u64, data.len() as u64) {
                            return Ok(media_status(Status::RateLimited));
                        }
                        MediaResponse {
                            data,
                            ..media_status(Status::Ok)
                        }
                    }
                    None => media_status(Status::NotFound),
                }
            }
            Some(media_request::Request::Attach(record)) => {
                if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
                    return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
                }
                let Ok((owner, attach)) =
                    owner_request::<proto::MediaAttach>(&record, Domain::MediaAttach, |_| true)
                else {
                    self.strike(peer);
                    return Ok(media_status(Status::Denied));
                };
                if self.listed(&owner) {
                    return Ok(media_status(Status::Refused));
                }
                let now = chrono::Utc::now().timestamp().unsigned_abs();
                if attach.release.len() != 32
                    || attach.hashes.is_empty()
                    || attach.hashes.len() > MAX_ATTACH
                    || attach.hashes.iter().any(|h| h.len() != 32)
                {
                    return Ok(media_status(Status::Invalid));
                }
                if attach.created > now.saturating_add(CLOCK_SKEW) {
                    return Ok(media_clock_invalid(now));
                }
                let expires = attach.created + u64::from(l.attachment_retention_hours) * 3600;
                match media.attach_signed(
                    &hex(&owner),
                    &attach,
                    expires,
                    MAX_ATTACHMENTS,
                    &record,
                )? {
                    Some(missing) => MediaResponse {
                        missing,
                        ..media_status(Status::Ok)
                    },
                    None => media_status(Status::Full),
                }
            }
            Some(media_request::Request::Release(release)) => {
                media_status(if media.release(&release.secret)? {
                    Status::Ok
                } else {
                    Status::NotFound
                })
            }
            Some(media_request::Request::ReplicaPut(replica)) => {
                if !self.media_charge(peer, replica.data.len() as u64, 0) {
                    return Ok(media_status(Status::RateLimited));
                }
                let response = self.store_media(&replica)?;
                if response.status == i32::from(Status::Denied) {
                    self.strike(peer);
                }
                response
            }
            Some(media_request::Request::ShardPut(put)) => {
                if !self.media_charge(peer, put.data.len() as u64, 0) {
                    return Ok(media_status(Status::RateLimited));
                }
                let response = self.store_shard(&put)?;
                if response.status == i32::from(Status::Denied) {
                    self.strike(peer);
                }
                response
            }
            Some(media_request::Request::Have(have)) => {
                if have.items.len() > crate::media_repair::MAX_ITEMS
                    || have
                        .items
                        .iter()
                        .map(|p| p.hash.as_slice())
                        .collect::<HashSet<_>>()
                        .len()
                        > MAX_KEEP
                    || have
                        .items
                        .iter()
                        .any(|p| p.hash.len() != 32 || p.index.is_some_and(|i| i >= 10))
                {
                    media_status(Status::Invalid)
                } else {
                    // The swarm applies trust and key proximity before reading inventory.
                    media_status(Status::Ok)
                }
            }
            Some(media_request::Request::ShardGet(get)) => {
                if get.hash.len() != 32 || get.index >= 10 {
                    return Ok(media_status(Status::Invalid));
                }
                if self.listed(&get.hash) {
                    return Ok(media_status(Status::Refused));
                }
                match media.shard(&get.hash, get.index as usize)? {
                    Some(data) if self.media_charge(peer, data.len() as u64, data.len() as u64) => {
                        MediaResponse {
                            data,
                            ..media_status(Status::Ok)
                        }
                    }
                    Some(_) => media_status(Status::RateLimited),
                    None => media_status(Status::NotFound),
                }
            }
            None => media_status(Status::Unsupported),
        })
    }

    /// Sorts `items` into present and missing, with the whole blobs and manifests held.
    pub fn media_inventory(&self, items: &[proto::MediaPart]) -> crate::Result<MediaResponse> {
        let media = self.media.as_ref().unwrap();
        let mut inventory = proto::MediaInventory::default();
        let mut hashes = HashSet::new();
        for part in items {
            if part.hash.len() != 32
                || part.index.is_some_and(|i| i >= 10)
                || self.listed(&part.hash)
            {
                continue;
            }
            if hashes.insert(part.hash.clone()) {
                if media.get(&part.hash)?.is_some_and(|d| {
                    d.len() <= crate::replication::MEDIA_OBJECT_BYTES
                        && dyapp_identity::sha256(&d).as_slice() == part.hash
                }) {
                    inventory.whole.push(part.hash.clone());
                } else if let Some(manifest) = media
                    .manifest(&part.hash)?
                    .filter(|m| m.hash == part.hash && m.data_shards().is_some())
                {
                    inventory.manifests.push(manifest);
                }
            }
            let whole = inventory.whole.contains(&part.hash);
            let manifest = inventory.manifests.iter().find(|m| m.hash == part.hash);
            let present = match part.index {
                None => whole || manifest.is_some(),
                Some(index) => {
                    if whole {
                        media.shard(&part.hash, index as usize)?.is_some()
                    } else if let Some(manifest) = manifest {
                        media
                            .shard(&part.hash, index as usize)?
                            .is_some_and(|d| manifest.accepts(index as usize, &d))
                    } else {
                        false
                    }
                }
            };
            let list = if present {
                &mut inventory.present
            } else {
                &mut inventory.missing
            };
            if !list.contains(part) {
                list.push(part.clone());
            }
        }
        Ok(MediaResponse {
            inventory: Some(inventory),
            ..media_status(Status::Ok)
        })
    }

    /// Stores a shard of a sharded media replica; the replica must be authorized.
    pub fn store_shard(&self, put: &proto::MediaShardPut) -> crate::Result<MediaResponse> {
        let Some(replica) = &put.replica else {
            return Ok(media_status(Status::Denied));
        };
        let Some(manifest) = &replica.manifest else {
            return Ok(media_status(Status::Invalid));
        };
        if !manifest.accepts(put.index as usize, &put.data) {
            return Ok(media_status(Status::Invalid));
        }
        let response = self.store_media(replica)?;
        if response.status != i32::from(Status::Ok) {
            return Ok(response);
        }
        Ok(media_status(
            match self
                .media
                .as_ref()
                .unwrap()
                .put_shard(manifest, put.index as usize, &put.data)?
            {
                Put::Stored => Status::Ok,
                Put::Invalid => Status::Invalid,
                Put::NotListed => Status::NotFound,
                Put::OverQuota => Status::Full,
            },
        ))
    }

    /// A blob from its whole copy, or decoded from `shards`;
    /// discards shards and manifest that do not decode.
    pub fn assemble_media(
        &self,
        manifest: &proto::MediaManifest,
        shards: &[(usize, Vec<u8>)],
    ) -> crate::Result<MediaResponse> {
        let media = self.media.as_ref().unwrap();
        if let Some(data) = media.get(&manifest.hash)? {
            if dyapp_identity::sha256(&data).as_slice() == manifest.hash {
                return Ok(MediaResponse {
                    data,
                    ..media_status(Status::Ok)
                });
            }
        }
        match crate::replication::media_decode(manifest, shards) {
            Ok(data) => Ok(MediaResponse {
                data,
                ..media_status(Status::Ok)
            }),
            Err(error) => {
                tracing::warn!(%error, "media decode failed");
                media.discard_sharded(&manifest.hash)?;
                Ok(media_status(Status::NotFound))
            }
        }
    }

    /// Charges reading and assembling a sharded blob to `peer`; false when refused.
    pub fn prepare_media_read(&self, peer: &Peer, manifest: &proto::MediaManifest) -> bool {
        let Some(k) = manifest.data_shards() else {
            return false;
        };
        let assembly =
            manifest.shard_size().unwrap() as u64 * (k + crate::replication::MEDIA_PARITY) as u64;
        self.media_charge(peer, manifest.length, manifest.length + assembly)
    }

    /// Byte charges are separate from the request-count buckets; 0 disables the byte cap.
    fn media_charge(&self, peer: &Peer, bytes: u64, reserve: u64) -> bool {
        let admitted = self.config.limits.bytes_per_second == 0
            || bytes == 0
            || self
                .media_bytes
                .check_units(&peer.id, u32::try_from(bytes).unwrap_or(u32::MAX));
        admitted && self.traffic.try_add(reserve, SHED_MEDIA)
    }

    /// Accepts one upload without storing locally unless this node holds a replica key.
    pub fn prepare_media(
        &self,
        peer: &Peer,
        put: &proto::MediaPut,
    ) -> crate::Result<std::result::Result<proto::MediaReplicaPut, Status>> {
        let Some(media) = &self.media else {
            return Ok(Err(Status::Unsupported));
        };
        let request = MediaRequest {
            request: Some(media_request::Request::Put(put.clone())),
        };
        if self.shed(request.encoded_len(), SHED_MEDIA)
            || !self.admit(peer)
            || !self.media_peers.check_limit(&peer.id)
        {
            return Ok(Err(Status::RateLimited));
        }
        if self.refused(peer)
            || self.listed(&put.owner)
            || self.listed(&dyapp_identity::sha256(&put.data))
        {
            return Ok(Err(Status::Refused));
        }
        if put.data.len() > MAX_MEDIA_BYTES {
            return Ok(Err(Status::TooLarge));
        }
        if put.owner.len() != 32 {
            return Ok(Err(Status::Invalid));
        }
        let hash = dyapp_identity::sha256(&put.data);
        let Some(authorization) = media.authorization(&hex(&put.owner), &hash)? else {
            return Ok(Err(Status::NotFound));
        };
        let bytes = put.data.len() as u64;
        let billed = if put.data.len() > self.config.media.shard_threshold {
            let k = bytes.div_ceil(crate::replication::MEDIA_OBJECT_BYTES as u64);
            (bytes * (k + crate::replication::MEDIA_PARITY as u64)).div_ceil(k)
        } else {
            bytes * u64::from(dyapp_p2p_net::REPLICAS)
        };
        if !self.media_charge(peer, billed, billed.saturating_sub(bytes)) {
            return Ok(Err(Status::RateLimited));
        }
        Ok(Ok(proto::MediaReplicaPut {
            data: put.data.clone(),
            authorization: Some(authorization),
            manifest: None,
        }))
    }

    /// Independently verifies the authorization and checks against the latest local keep.
    pub fn store_media(&self, replica: &proto::MediaReplicaPut) -> crate::Result<MediaResponse> {
        use proto::media_replica_put::Authorization;
        let Some(media) = &self.media else {
            return Ok(media_status(Status::Unsupported));
        };
        if replica.data.len() > crate::replication::MEDIA_OBJECT_BYTES {
            return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
        }
        let hash = if let Some(manifest) = &replica.manifest {
            if !replica.data.is_empty() || manifest.data_shards().is_none() {
                return Ok(media_status(Status::Invalid));
            }
            manifest.hash.clone()
        } else {
            dyapp_identity::sha256(&replica.data).to_vec()
        };
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        let l = &self.config.limits;
        let owner = match &replica.authorization {
            Some(Authorization::Keep(record)) => {
                if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
                    return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
                }
                let Ok((owner, keep)) =
                    owner_request::<proto::MediaKeep>(record, Domain::MediaKeep, |_| true)
                else {
                    return Ok(media_status(Status::Denied));
                };
                if keep.version == 0
                    || keep.hashes.len() > MAX_KEEP
                    || keep.hashes.iter().any(|h| h.len() != 32)
                {
                    return Ok(media_status(Status::Invalid));
                }
                if keep.time > now.saturating_add(CLOCK_SKEW)
                    || now.saturating_sub(keep.time) > u64::from(l.profile_ttl_days) * 86400
                {
                    return Ok(media_clock_invalid(now));
                }
                if self.listed(&owner) || self.listed(&hash) {
                    return Ok(media_status(Status::Refused));
                }
                if !keep.hashes.iter().any(|h| h.as_slice() == hash) {
                    return Ok(media_status(Status::NotFound));
                }
                media.keep_signed(&hex(&owner), &keep, record)?;
                if !media.kept(&hex(&owner), &hash)? {
                    return Ok(media_status(Status::NotFound));
                }
                owner
            }
            Some(Authorization::Attach(record)) => {
                if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
                    return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
                }
                let Ok((owner, attach)) =
                    owner_request::<proto::MediaAttach>(record, Domain::MediaAttach, |_| true)
                else {
                    return Ok(media_status(Status::Denied));
                };
                let expires = attach
                    .created
                    .saturating_add(u64::from(l.attachment_retention_hours) * 3600);
                if attach.release.len() != 32
                    || attach.hashes.is_empty()
                    || attach.hashes.len() > MAX_ATTACH
                    || attach.hashes.iter().any(|h| h.len() != 32)
                {
                    return Ok(media_status(Status::Invalid));
                }
                if attach.created > now.saturating_add(CLOCK_SKEW) || expires <= now {
                    return Ok(media_clock_invalid(now));
                }
                if self.listed(&owner) || self.listed(&hash) {
                    return Ok(media_status(Status::Refused));
                }
                if !attach.hashes.iter().any(|h| h.as_slice() == hash) {
                    return Ok(media_status(Status::NotFound));
                }
                if media
                    .attach_signed(&hex(&owner), &attach, expires, MAX_ATTACHMENTS, record)?
                    .is_none()
                {
                    return Ok(media_status(Status::Full));
                }
                owner
            }
            None => return Ok(media_status(Status::Denied)),
        };
        let max = l.media_max_mb.saturating_mul(1 << 20);
        let (mut used, mut free) = media.usage()?;
        if used >= max {
            let removed = media.evict(max / 100 * 95)?;
            tracing::info!(removed, "media lists evicted");
            (used, free) = media.usage()?;
        }
        if self.guard(
            2,
            used >= max || free <= l.min_free_mb.saturating_mul(1 << 20),
        ) {
            return Ok(media_status(Status::Full));
        }
        // Whole-copy ownership is charged at R=5 even when keys co-locate on a small network.
        let quota = l.media_per_owner_mb.saturating_mul(1 << 20);
        let stored = if let Some(manifest) = &replica.manifest {
            media.put_manifest(&hex(&owner), manifest, quota)?
        } else {
            media.put(
                &hex(&owner),
                &replica.data,
                quota / u64::from(dyapp_p2p_net::REPLICAS),
            )?
        };
        Ok(media_status(match stored {
            Put::Stored => Status::Ok,
            Put::NotListed => Status::NotFound,
            Put::OverQuota => Status::Full,
            Put::Invalid => Status::Invalid,
        }))
    }
}

fn media_clock_invalid(server_time: u64) -> MediaResponse {
    MediaResponse {
        server_time,
        ..media_status(Status::Invalid)
    }
}

/// Converts prepare_media's upload rejection without changing its Status interface.
pub(crate) fn media_upload_status(status: Status) -> MediaResponse {
    if status == Status::TooLarge {
        media_too_large(MAX_MEDIA_BYTES)
    } else {
        media_status(status)
    }
}

fn media_too_large(max: usize) -> MediaResponse {
    MediaResponse {
        max_bytes: max as u64,
        ..media_status(Status::TooLarge)
    }
}

fn media_status(status: Status) -> MediaResponse {
    MediaResponse {
        status: status.into(),
        ..MediaResponse::default()
    }
}
