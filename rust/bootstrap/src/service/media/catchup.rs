//! Node-only reverse ancestry; no private ownership snapshots are returned to replicas.
use super::{
    hex, media_clock_invalid, media_status, media_too_large, owner_request, proto, Domain,
    MediaResponse, Message, Peer, Service, Status, CLOCK_SKEW,
};
use crate::config::Limits;
use crate::media::{KeepHistoryProgress, MediaStore};
use crate::{BootstrapError, Result};
use dyapp_identity::SignedRecord;
use std::ops::RangeInclusive;

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

fn matches_target(
    media: &MediaStore,
    owner: &str,
    target: &SignedRecord,
    version: u64,
) -> Result<bool> {
    let current = media.keep_snapshot(owner)?;
    Ok(current.owner_present
        && current.version == version
        && current.record.as_ref() == Some(target))
}

fn apply_segment(
    media: &MediaStore,
    target: &SignedRecord,
    ancestors: &[SignedRecord],
    fresh: RangeInclusive<u64>,
    limits: &Limits,
) -> Result<KeepHistoryProgress> {
    if media
        .stage_keep_history(target, ancestors, fresh.clone(), limits)?
        .is_none()
    {
        return Ok(KeepHistoryProgress::Conflict);
    }
    media.apply_keep_history(target, fresh, limits)
}

fn progress(
    result: Result<KeepHistoryProgress>,
    media: &MediaStore,
    owner: &str,
    target: &SignedRecord,
    version: u64,
) -> Result<MediaResponse> {
    let (ready, next_version, target_current) = match result {
        Ok(KeepHistoryProgress::Ready { next_version }) => (
            true,
            next_version,
            Some(matches_target(media, owner, target, version)?),
        ),
        Ok(KeepHistoryProgress::Pending { next_version }) => (false, next_version, None),
        Ok(KeepHistoryProgress::Conflict) => return Ok(media_status(Status::Stale)),
        Err(BootstrapError::MediaHistoryFull) => return Ok(media_status(Status::Full)),
        Err(BootstrapError::InvalidRequest(_)) => return Ok(media_status(Status::Invalid)),
        Err(error) => return Err(error),
    };
    Ok(MediaResponse {
        keep_progress: Some(proto::MediaKeepProgress {
            ready,
            next_version,
            target_current,
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
        let result = apply_segment(
            media,
            target,
            &segment.ancestors,
            fresh,
            &self.config.limits,
        );
        progress(result, media, &hex(&owner), target, head.version)
    }
}
