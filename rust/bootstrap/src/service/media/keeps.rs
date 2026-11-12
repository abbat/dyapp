//! Origin keep admission. Conditional receipts bind the exact signed request and expectation.
use super::{
    hex, media_clock_invalid, media_status, media_too_large, owner_request, proto, Domain,
    MediaResponse, Message, Peer, Service, Status, CLOCK_SKEW, MAX_KEEP,
};
use crate::media::{KeepSnapshot, KeepUpdate, MediaStore};
use dyapp_identity::SignedRecord;

pub(super) fn projection(keep: &proto::MediaKeepIntent) -> proto::MediaKeep {
    proto::MediaKeep {
        version: keep.version,
        hashes: keep.hashes.clone(),
        time: keep.time,
    }
}

pub(super) fn valid_condition(condition: &proto::MediaKeepCondition) -> bool {
    condition.version <= i64::MAX.cast_unsigned()
        && matches!(condition.record_hash.len(), 0 | 32)
        && (condition.owner_present || (condition.version == 0 && condition.record_hash.is_empty()))
        && (condition.record_hash.is_empty() || condition.version > 0)
}

pub(super) fn valid_ownership(keep: &proto::MediaKeepIntent) -> bool {
    if keep.version == 0
        || keep.version > i64::MAX.cast_unsigned()
        || keep.hashes.len() > MAX_KEEP
        || keep.hashes.iter().any(|hash| hash.len() != 32)
    {
        return false;
    }
    match &keep.expected {
        Some(expected) => {
            valid_condition(expected)
                && expected.version.checked_add(1) == Some(keep.version)
                && keep.intent_hash.len() == 32
                && keep.requester.parse::<libp2p::PeerId>().is_ok()
        }
        None => keep.intent_hash.is_empty() && !keep.pending && keep.requester.is_empty(),
    }
}

pub(super) fn decode_authorization(
    record: &SignedRecord,
) -> Result<([u8; 32], proto::MediaKeepIntent), ()> {
    let keep = proto::MediaKeepIntent::decode(record.payload.as_slice()).map_err(|_| ())?;
    let domain = if keep.expected.is_some() {
        Domain::MediaConditionalKeep
    } else {
        Domain::MediaKeep
    };
    owner_request(record, domain, |_| true)
}

fn wire_state(state: KeepSnapshot) -> proto::MediaKeepState {
    proto::MediaKeepState {
        owner_present: state.owner_present,
        version: state.version,
        record: state.record,
    }
}

fn condition_matches(expected: &proto::MediaKeepCondition, state: &KeepSnapshot) -> bool {
    state.matches(expected)
}

fn state_reply(status: Status, state: KeepSnapshot) -> MediaResponse {
    MediaResponse {
        keep_state: Some(Box::new(wire_state(state))),
        ..media_status(status)
    }
}

fn apply_condition(
    media: &MediaStore,
    owner: &str,
    keep: &proto::MediaKeepIntent,
    expected: &proto::MediaKeepCondition,
    record: &SignedRecord,
    limits: &crate::config::Limits,
) -> crate::Result<MediaResponse> {
    if !valid_ownership(keep) {
        return Ok(media_status(Status::Invalid));
    }
    let current = media.keep_snapshot(owner)?;
    if !condition_matches(expected, &current) {
        return Ok(state_reply(Status::Stale, current));
    }
    let outcome =
        match media.replace_keep_with_limits(owner, &current, &projection(keep), record, limits) {
            Err(crate::error::BootstrapError::MediaHistoryFull) => {
                return Ok(media_status(Status::Full))
            }
            other => other?,
        };
    match outcome {
        KeepUpdate::Applied(missing) => Ok(MediaResponse {
            keep_applied: true,
            missing,
            keep_state: Some(Box::new(proto::MediaKeepState {
                owner_present: true,
                version: keep.version,
                record: Some(record.clone()),
            })),
            ..media_status(Status::Ok)
        }),
        KeepUpdate::Conflict(current) => Ok(state_reply(Status::Stale, current)),
    }
}

impl Service {
    pub(super) fn legacy_keep(
        &self,
        peer: &Peer,
        media: &MediaStore,
        record: &SignedRecord,
    ) -> crate::Result<MediaResponse> {
        if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
            return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
        }
        let Ok((owner, keep)) =
            owner_request::<proto::MediaKeep>(record, Domain::MediaKeep, |_| true)
        else {
            self.strike(peer);
            return Ok(media_status(Status::Denied));
        };
        if self.listed(&owner) {
            return Ok(media_status(Status::Refused));
        }
        if keep.version == 0
            || keep.version > i64::MAX.cast_unsigned()
            || keep.hashes.len() > MAX_KEEP
            || keep.hashes.iter().any(|hash| hash.len() != 32)
        {
            return Ok(media_status(Status::Invalid));
        }
        let now = self.clock.unix_secs();
        if keep.time.abs_diff(now) > CLOCK_SKEW {
            return Ok(media_clock_invalid(now));
        }
        Ok(match media.keep_signed(&hex(&owner), &keep, record)? {
            Some(missing) => MediaResponse {
                missing,
                ..media_status(Status::Ok)
            },
            None => media_status(Status::Stale),
        })
    }

    pub(super) fn conditional_keep(
        &self,
        peer: &Peer,
        media: &MediaStore,
        record: &SignedRecord,
    ) -> crate::Result<MediaResponse> {
        if record.encoded_len() > crate::replication::MEDIA_OBJECT_BYTES {
            return Ok(media_too_large(crate::replication::MEDIA_OBJECT_BYTES));
        }
        let Ok((owner, keep)) =
            owner_request::<proto::MediaKeepIntent>(record, Domain::MediaConditionalKeep, |keep| {
                keep.requester == peer.id
            })
        else {
            self.strike(peer);
            return Ok(media_status(Status::Denied));
        };
        if self.listed(&owner) {
            return Ok(media_status(Status::Refused));
        }
        let now = self.clock.unix_secs();
        if keep.time.abs_diff(now) > CLOCK_SKEW {
            return Ok(media_clock_invalid(now));
        }
        let owner = hex(&owner);
        let Some(expected) = &keep.expected else {
            if keep.version != 0
                || !keep.hashes.is_empty()
                || !keep.intent_hash.is_empty()
                || keep.pending
            {
                return Ok(media_status(Status::Invalid));
            }
            return Ok(state_reply(Status::Ok, media.keep_snapshot(&owner)?));
        };
        apply_condition(media, &owner, &keep, expected, record, &self.config.limits)
    }
}
