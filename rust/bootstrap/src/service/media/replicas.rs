//! Replica proofs retain the complete signed ownership metadata and obey the local latest list.
use super::keeps::{decode_authorization, projection, valid_ownership};
use super::{
    hex, media_clock_invalid, media_status, media_too_large, owner_request, proto, Attached,
    Domain, HashSet, MediaManifestExt, MediaResponse, Message, Put, Service, Status, CLOCK_SKEW,
    MAX_ATTACH, MAX_ATTACHMENTS,
};
use crate::media::MediaStore;
use dyapp_identity::SignedRecord;

type Admission = crate::Result<Result<[u8; 32], MediaResponse>>;

fn replica_hash(replica: &proto::MediaReplicaPut) -> Result<Vec<u8>, Status> {
    if replica.data.len() > crate::replication::MEDIA_OBJECT_BYTES {
        return Err(Status::TooLarge);
    }
    if let Some(manifest) = &replica.manifest {
        if !replica.data.is_empty() || manifest.data_shards().is_none() {
            return Err(Status::Invalid);
        }
        Ok(manifest.hash.clone())
    } else {
        Ok(dyapp_identity::sha256(&replica.data).to_vec())
    }
}

impl Service {
    pub(super) fn store_media_with_deny(
        &self,
        replica: &proto::MediaReplicaPut,
        deny: &HashSet<String>,
    ) -> crate::Result<MediaResponse> {
        let Some(media) = &self.media else {
            return Ok(media_status(Status::Unsupported));
        };
        let hash = match replica_hash(replica) {
            Ok(hash) => hash,
            Err(Status::TooLarge) => {
                return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
            }
            Err(status) => return Ok(media_status(status)),
        };
        let owner = match self.media_authorization(media, replica, &hash, deny)? {
            Ok(owner) => owner,
            Err(response) => return Ok(response),
        };
        if !self.media_capacity(media)? {
            return Ok(media_status(Status::Full));
        }
        let quota = self
            .config
            .limits
            .media_per_owner_mb
            .saturating_mul(1 << 20);
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

    fn media_capacity(&self, media: &MediaStore) -> crate::Result<bool> {
        let limits = &self.config.limits;
        let max = limits.media_max_mb.saturating_mul(1 << 20);
        let (mut used, mut free) = media.usage()?;
        if used >= max {
            let removed = media.evict(max / 100 * 95)?;
            tracing::info!(removed, "media lists evicted");
            (used, free) = media.usage()?;
        }
        Ok(!self.guard(
            2,
            used >= max || free <= limits.min_free_mb.saturating_mul(1 << 20),
        ))
    }

    fn media_authorization(
        &self,
        media: &MediaStore,
        replica: &proto::MediaReplicaPut,
        hash: &[u8],
        deny: &HashSet<String>,
    ) -> Admission {
        use proto::media_replica_put::Authorization;
        match &replica.authorization {
            Some(Authorization::Keep(record)) => self.keep_authorization(media, record, hash, deny),
            Some(Authorization::Attach(record)) => {
                self.attach_authorization(media, record, hash, deny)
            }
            None => Ok(Err(media_status(Status::Denied))),
        }
    }

    fn keep_authorization(
        &self,
        media: &MediaStore,
        record: &SignedRecord,
        hash: &[u8],
        deny: &HashSet<String>,
    ) -> Admission {
        if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
            return Ok(Err(media_too_large(crate::replication::MEDIA_OBJECT_BYTES)));
        }
        let Ok((owner, keep)) = decode_authorization(record) else {
            return Ok(Err(media_status(Status::Denied)));
        };
        if !valid_ownership(&keep) {
            return Ok(Err(media_status(Status::Invalid)));
        }
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        if keep.time > now.saturating_add(CLOCK_SKEW)
            || now.saturating_sub(keep.time)
                > u64::from(self.config.limits.profile_ttl_days) * 86400
        {
            return Ok(Err(media_clock_invalid(now)));
        }
        if deny.contains(&hex(&owner)) || deny.contains(&hex(hash)) {
            return Ok(Err(media_status(Status::Refused)));
        }
        if !keep.hashes.iter().any(|held| held.as_slice() == hash) {
            return Ok(Err(media_status(Status::NotFound)));
        }
        match media.keep_signed_with_limits(
            &hex(&owner),
            &projection(&keep),
            record,
            &self.config.limits,
        ) {
            Err(crate::error::BootstrapError::MediaHistoryFull) => {
                return Ok(Err(media_status(Status::Full)))
            }
            other => {
                other?;
            }
        }
        if !media.kept(&hex(&owner), hash)? {
            return Ok(Err(media_status(Status::NotFound)));
        }
        Ok(Ok(owner))
    }

    fn attach_authorization(
        &self,
        media: &MediaStore,
        record: &SignedRecord,
        hash: &[u8],
        deny: &HashSet<String>,
    ) -> Admission {
        if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
            return Ok(Err(media_too_large(crate::replication::MEDIA_OBJECT_BYTES)));
        }
        let Ok((owner, attach)) =
            owner_request::<proto::MediaAttach>(record, Domain::MediaAttach, |_| true)
        else {
            return Ok(Err(media_status(Status::Denied)));
        };
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        let expires = attach
            .created
            .saturating_add(u64::from(self.config.limits.attachment_retention_hours) * 3600);
        if attach.release.len() != 32
            || attach.hashes.is_empty()
            || attach.hashes.len() > MAX_ATTACH
            || attach.hashes.iter().any(|held| held.len() != 32)
        {
            return Ok(Err(media_status(Status::Invalid)));
        }
        if attach.created > now.saturating_add(CLOCK_SKEW) || expires <= now {
            return Ok(Err(media_clock_invalid(now)));
        }
        if deny.contains(&hex(&owner)) || deny.contains(&hex(hash)) {
            return Ok(Err(media_status(Status::Refused)));
        }
        if !attach.hashes.iter().any(|held| held.as_slice() == hash) {
            return Ok(Err(media_status(Status::NotFound)));
        }
        match media.attach_signed(&hex(&owner), &attach, expires, MAX_ATTACHMENTS, record)? {
            Attached::Missing(_) => {}
            Attached::Full => return Ok(Err(media_status(Status::Full))),
            Attached::Released => return Ok(Err(media_status(Status::Refused))),
        }
        Ok(Ok(owner))
    }
}
