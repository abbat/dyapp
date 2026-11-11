//! The media role's requests and replica puts.
#[path = "media/catchup.rs"]
mod catchup;
#[path = "media/keeps.rs"]
mod keeps;
#[path = "media/replicas.rs"]
mod replicas;
#[path = "media/requests.rs"]
mod requests;

use super::{
    hex, media_request, owner_request, proto, Attached, Domain, HashSet, MediaManifestExt,
    MediaRequest, MediaResponse, Message, Peer, PoisonError, Put, Service, Status, CLOCK_SKEW,
    MAX_ATTACH, MAX_ATTACHMENTS, MAX_KEEP, MAX_MEDIA_BYTES, SHED_MEDIA,
};

impl Service {
    /// Errors as in [`Service::profile`].
    ///
    /// # Errors
    ///
    /// Returns an error if media storage, ownership lookup, blob/cache access, inventory or
    /// replica persistence fails. Admission and invalid-request outcomes use response statuses.
    pub fn media(&self, peer: &Peer, request: MediaRequest) -> crate::Result<MediaResponse> {
        if let Some(media_request::Request::Put(put)) = &request.request {
            return match self.prepare_media(peer, put)? {
                Ok(replica) => self.store_media(&replica),
                Err(status) => Ok(media_upload_status(status)),
            };
        }
        if self.media.is_none() {
            return Ok(media_status(Status::Unsupported));
        }
        let is_replica = matches!(
            request.request,
            Some(
                media_request::Request::ReplicaPut(_)
                    | media_request::Request::ShardPut(_)
                    | media_request::Request::KeepCatchup(_)
            )
        );
        if is_replica && !self.node_peer(peer) {
            self.strike(peer);
            return Ok(media_status(Status::Denied));
        }
        // Media refusal is not a strike: a client loading a gallery is not misbehaving.
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
        self.media_request(peer, request.request)
    }

    /// Sorts `items` into present and missing, with the whole blobs and manifests held.
    ///
    /// # Errors
    ///
    /// Returns an error if stored blobs, manifests or shards cannot be read or decoded,
    /// or a fallback shard cannot be encoded.
    ///
    /// # Panics
    ///
    /// Panics if this service has no media store; callers must first check that media is enabled.
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
            let present = part_present(media, &inventory, part)?;
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
    ///
    /// # Errors
    ///
    /// Returns an error if replica authorization storage or manifest/shard persistence fails.
    /// Invalid or refused replicas use response statuses.
    ///
    /// # Panics
    ///
    /// Panics if an accepted replica reaches shard persistence without a media store.
    /// The preceding replica-storage status check prevents that state.
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
        let deny = self.deny.read().unwrap_or_else(PoisonError::into_inner);
        let response = self.store_media_with_deny(replica, &deny)?;
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

    /// The piece `get` asks for of a blob from its whole copy, or decoded from `shards` and
    /// cached for the next pieces; discards shards and manifest that do not decode.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the whole blob or discarding undecodable sharded content fails.
    /// A cache-write failure is logged without discarding a successfully reconstructed blob.
    ///
    /// # Panics
    ///
    /// Panics if assembly reaches storage access without a media store; callers must first
    /// check that media is enabled.
    pub fn assemble_media(
        &self,
        manifest: &proto::MediaManifest,
        shards: &[(usize, Vec<u8>)],
        get: &proto::GetMedia,
    ) -> crate::Result<MediaResponse> {
        if self.listed(&manifest.hash) {
            return Ok(media_status(Status::Refused));
        }
        let media = self.media.as_ref().unwrap();
        if let Some(data) = media.get(&manifest.hash)? {
            if dyapp_identity::sha256(&data).as_slice() == manifest.hash {
                return Ok(media_piece(get, data));
            }
        }
        match crate::replication::media_decode(manifest, shards) {
            Ok(data) => {
                cache_assembled(media, &manifest.hash, &data);
                Ok(media_piece(get, data))
            }
            Err(error) => {
                tracing::warn!(%error, "media decode failed");
                media.discard_sharded(&manifest.hash)?;
                Ok(media_status(Status::NotFound))
            }
        }
    }

    /// Charges reading the piece `get` asks for and assembling a sharded blob to `peer`; false
    /// when refused.
    ///
    /// # Panics
    ///
    /// Panics if a manifest accepted by `data_shards` has no shard size, which would violate
    /// the shared manifest-shape invariant.
    pub fn prepare_media_read(
        &self,
        peer: &Peer,
        manifest: &proto::MediaManifest,
        get: &proto::GetMedia,
    ) -> bool {
        if self.listed(&manifest.hash) {
            return false;
        }
        let Some(k) = manifest.data_shards() else {
            return false;
        };
        let assembly =
            manifest.shard_size().unwrap() as u64 * (k + crate::replication::MEDIA_PARITY) as u64;
        let piece = match get.len {
            0 => manifest.length,
            len => len.min(manifest.length.saturating_sub(get.offset)),
        };
        self.media_charge(peer, piece, piece + assembly)
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
    ///
    /// # Errors
    ///
    /// Returns an error if the stored media authorization cannot be read or decoded.
    /// Admission and invalid-upload outcomes use the inner status.
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
    ///
    /// # Errors
    ///
    /// Returns an error if ownership/attachment records, blob data or manifests cannot be
    /// stored or read. Invalid or refused replicas use response statuses.
    pub fn store_media(&self, replica: &proto::MediaReplicaPut) -> crate::Result<MediaResponse> {
        let deny = self.deny.read().unwrap_or_else(PoisonError::into_inner);
        self.store_media_with_deny(replica, &deny)
    }
}

fn cache_assembled(media: &crate::media::MediaStore, hash: &[u8], data: &[u8]) {
    let now = chrono::Utc::now().timestamp();
    if let Err(error) = media.cache(hash, data, now) {
        tracing::error!(%error, "media cache write failed");
    }
}

fn part_present(
    media: &crate::media::MediaStore,
    inventory: &proto::MediaInventory,
    part: &proto::MediaPart,
) -> crate::Result<bool> {
    let whole = inventory.whole.contains(&part.hash);
    let manifest = inventory
        .manifests
        .iter()
        .find(|manifest| manifest.hash == part.hash);
    Ok(match part.index {
        None => whole || manifest.is_some(),
        Some(index) => {
            if whole {
                media.shard(&part.hash, index as usize)?.is_some()
            } else if let Some(manifest) = manifest {
                media
                    .shard(&part.hash, index as usize)?
                    .is_some_and(|data| manifest.accepts(index as usize, &data))
            } else {
                false
            }
        }
    })
}

fn media_clock_invalid(server_time: u64) -> MediaResponse {
    MediaResponse {
        server_time,
        ..media_status(Status::Invalid)
    }
}

/// Converts `prepare_media`'s upload rejection without changing its Status interface.
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

/// The piece of `data` that `get` asks for, with the blob size; `INVALID` past its end.
fn media_piece(get: &proto::GetMedia, mut data: Vec<u8>) -> MediaResponse {
    let total_size = data.len() as u64;
    if get.len != 0 {
        if get.offset >= total_size {
            return media_status(Status::Invalid);
        }
        let end = get.offset.saturating_add(get.len).min(total_size);
        let (Ok(start), Ok(end)) = (usize::try_from(get.offset), usize::try_from(end)) else {
            return media_status(Status::Invalid);
        };
        data = data[start..end].to_vec();
    }
    MediaResponse {
        data,
        total_size,
        ..media_status(Status::Ok)
    }
}
