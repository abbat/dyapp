//! Node-only reverse ancestry; no private ownership snapshots are returned to replicas.
use super::{
    media_clock_invalid, media_status, media_too_large, owner_request, proto, Domain,
    MediaResponse, Message, Peer, Service, Status, CLOCK_SKEW,
};
use crate::media::{KeepHistoryProgress, MediaStore};
use crate::{BootstrapError, Result};

fn framed(bytes: usize) -> usize {
    1 + prost::encoding::encoded_len_varint(bytes as u64) + bytes
}

fn bounded(segment: &proto::MediaKeepCatchup) -> bool {
    let Some(target) = &segment.target else {
        return false;
    };
    let record_limit = crate::replication::MEDIA_OBJECT_BYTES;
    segment.ancestors.len() <= 256
        && target.encoded_len() <= record_limit
        && segment
            .ancestors
            .iter()
            .all(|record| record.encoded_len() <= record_limit)
        && segment.encoded_len()
            <= MediaStore::keep_history_page_bytes() + framed(target.encoded_len())
}

fn progress(result: Result<KeepHistoryProgress>) -> Result<MediaResponse> {
    let (ready, next_version) = match result {
        Ok(KeepHistoryProgress::Ready { next_version }) => (true, next_version),
        Ok(KeepHistoryProgress::Pending { next_version }) => (false, next_version),
        Ok(KeepHistoryProgress::Conflict) => return Ok(media_status(Status::Stale)),
        Err(BootstrapError::MediaHistoryFull) => return Ok(media_status(Status::Full)),
        Err(BootstrapError::InvalidRequest(_)) => return Ok(media_status(Status::Invalid)),
        Err(error) => return Err(error),
    };
    Ok(MediaResponse {
        keep_progress: Some(proto::MediaKeepProgress {
            ready,
            next_version,
        }),
        ..media_status(Status::Ok)
    })
}

impl Service {
    pub(super) fn media_keep_catchup(
        &self,
        peer: &Peer,
        media: &MediaStore,
        segment: &proto::MediaKeepCatchup,
    ) -> Result<MediaResponse> {
        let Some(target) = &segment.target else {
            return Ok(media_status(Status::Invalid));
        };
        if !bounded(segment) {
            return Ok(media_too_large(
                MediaStore::keep_history_page_bytes()
                    + framed(crate::replication::MEDIA_OBJECT_BYTES),
            ));
        }
        if !self.media_charge(peer, segment.encoded_len() as u64, 0) {
            return Ok(media_status(Status::RateLimited));
        }
        let Ok((owner, head)) =
            owner_request::<proto::MediaKeepIntent>(target, Domain::MediaConditionalKeep, |_| true)
        else {
            self.strike(peer);
            return Ok(media_status(Status::Denied));
        };
        if self.listed(&owner) || head.hashes.iter().any(|hash| self.listed(hash)) {
            return Ok(media_status(Status::Refused));
        }
        let now = chrono::Utc::now().timestamp().unsigned_abs();
        let lower = now.saturating_sub(u64::from(self.config.limits.profile_ttl_days) * 86400);
        let fresh = lower..=now.saturating_add(CLOCK_SKEW);
        if !fresh.contains(&head.time) {
            return Ok(media_clock_invalid(now));
        }
        let free = media.usage()?.1;
        if self.guard(
            2,
            free <= self.config.limits.min_free_mb.saturating_mul(1 << 20),
        ) {
            return Ok(media_status(Status::Full));
        }
        // Requester binding is checked at the origin. A node relays the unchanged signed target.
        match media.stage_keep_history(
            target,
            &segment.ancestors,
            fresh.clone(),
            &self.config.limits,
        ) {
            Ok(Some(_)) => {}
            Ok(None) => return Ok(media_status(Status::Stale)),
            Err(error) => return progress(Err(error)),
        }
        progress(media.apply_keep_history(target, fresh, &self.config.limits))
    }
}
