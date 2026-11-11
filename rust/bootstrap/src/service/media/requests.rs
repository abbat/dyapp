use super::{
    hex, media_clock_invalid, media_piece, media_status, media_too_large, owner_request, proto,
    Attached, Domain, HashSet, MediaResponse, Message, Peer, Service, Status, CLOCK_SKEW,
    MAX_ATTACH, MAX_ATTACHMENTS, MAX_KEEP,
};
use crate::media::MediaStore;
use proto::media_request::Request;

impl Service {
    pub(super) fn media_request(
        &self,
        peer: &Peer,
        request: Option<Request>,
    ) -> crate::Result<MediaResponse> {
        let Some(media) = &self.media else {
            return Ok(media_status(Status::Unsupported));
        };
        match request {
            Some(Request::Keep(record)) => self.legacy_keep(peer, media, &record),
            Some(Request::ConditionalKeep(record)) => self.conditional_keep(peer, media, &record),
            Some(Request::Put(_)) => unreachable!("put admitted separately"),
            Some(Request::Get(get)) => self.media_get(peer, media, &get),
            Some(Request::Attach(record)) => self.media_attach(peer, media, &record),
            Some(Request::Release(release)) => {
                Ok(media_status(if media.release(&release.secret)? {
                    Status::Ok
                } else {
                    Status::NotFound
                }))
            }
            Some(Request::ReplicaPut(replica)) => {
                self.media_upload_request(peer, replica.data.len(), || self.store_media(&replica))
            }
            Some(Request::ShardPut(put)) => {
                self.media_upload_request(peer, put.data.len(), || self.store_shard(&put))
            }
            Some(Request::Have(have)) => Ok(inventory_status(&have)),
            Some(Request::ShardGet(get)) => self.media_shard_get(peer, media, &get),
            None => Ok(media_status(Status::Unsupported)),
        }
    }

    fn media_upload_request(
        &self,
        peer: &Peer,
        bytes: usize,
        write: impl FnOnce() -> crate::Result<MediaResponse>,
    ) -> crate::Result<MediaResponse> {
        if !self.media_charge(peer, bytes as u64, 0) {
            return Ok(media_status(Status::RateLimited));
        }
        let response = write()?;
        if response.status == i32::from(Status::Denied) {
            self.strike(peer);
        }
        Ok(response)
    }

    fn media_get(
        &self,
        peer: &Peer,
        media: &MediaStore,
        get: &proto::GetMedia,
    ) -> crate::Result<MediaResponse> {
        if get.hash.len() != 32
            || get.len > crate::replication::MEDIA_OBJECT_BYTES as u64
            || (get.len == 0 && get.offset != 0)
        {
            return Ok(media_status(Status::Invalid));
        }
        if self.listed(&get.hash) {
            return Ok(media_status(Status::Refused));
        }
        let now = chrono::Utc::now().timestamp();
        let data = match media.get(&get.hash)? {
            Some(data) => Some(data),
            None => media.cached(&get.hash, now)?,
        };
        Ok(match data {
            Some(data) => {
                let response = media_piece(get, data);
                let len = response.data.len() as u64;
                if !self.media_charge(peer, len, len) {
                    return Ok(media_status(Status::RateLimited));
                }
                response
            }
            None => media_status(Status::NotFound),
        })
    }

    fn media_attach(
        &self,
        peer: &Peer,
        media: &MediaStore,
        record: &dyapp_identity::SignedRecord,
    ) -> crate::Result<MediaResponse> {
        if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
            return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
        }
        let Ok((owner, attach)) =
            owner_request::<proto::MediaAttach>(record, Domain::MediaAttach, |_| true)
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
            || attach.hashes.iter().any(|hash| hash.len() != 32)
        {
            return Ok(media_status(Status::Invalid));
        }
        if attach.created > now.saturating_add(CLOCK_SKEW) {
            return Ok(media_clock_invalid(now));
        }
        let expires =
            attach.created + u64::from(self.config.limits.attachment_retention_hours) * 3600;
        Ok(
            match media.attach_signed(&hex(&owner), &attach, expires, MAX_ATTACHMENTS, record)? {
                Attached::Missing(missing) => MediaResponse {
                    missing,
                    ..media_status(Status::Ok)
                },
                Attached::Full => media_status(Status::Full),
                Attached::Released => media_status(Status::Refused),
            },
        )
    }

    fn media_shard_get(
        &self,
        peer: &Peer,
        media: &MediaStore,
        get: &proto::MediaShardGet,
    ) -> crate::Result<MediaResponse> {
        if get.hash.len() != 32 || get.index >= 10 {
            return Ok(media_status(Status::Invalid));
        }
        if self.listed(&get.hash) {
            return Ok(media_status(Status::Refused));
        }
        Ok(match media.shard(&get.hash, get.index as usize)? {
            Some(data) if self.media_charge(peer, data.len() as u64, data.len() as u64) => {
                MediaResponse {
                    data,
                    ..media_status(Status::Ok)
                }
            }
            Some(_) => media_status(Status::RateLimited),
            None => media_status(Status::NotFound),
        })
    }
}

fn inventory_status(have: &proto::MediaHave) -> MediaResponse {
    if have.items.len() > crate::media_repair::MAX_ITEMS
        || have
            .items
            .iter()
            .map(|part| part.hash.as_slice())
            .collect::<HashSet<_>>()
            .len()
            > MAX_KEEP
        || have
            .items
            .iter()
            .any(|part| part.hash.len() != 32 || part.index.is_some_and(|index| index >= 10))
    {
        media_status(Status::Invalid)
    } else {
        // The swarm applies trust and key proximity before reading inventory.
        media_status(Status::Ok)
    }
}
