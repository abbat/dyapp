//! Owner-authenticated media snapshots and signed conditional keep requests.
use crate::{now, ok, Error, Node};
use dyapp_identity::{sha256, Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{self, media_request, Status};
use prost::Message;

#[cfg(test)]
mod tests;

const MAX_KEEP: usize = 256;
const MAX_RECORD: usize = 1 << 20;

/// A node's ownership state with every signed record verified against this owner.
#[derive(Clone, Debug)]
pub struct State {
    owner_key: [u8; 32],
    wire: proto::MediaKeepState,
    projection: Option<proto::MediaKeepIntent>,
}

/// The exact public-profile target of a staged or finalized media list.
#[derive(Clone, Copy, Debug)]
pub struct Intent {
    /// SHA-256 of the complete signed profile record.
    pub profile_hash: [u8; 32],
    /// Whether publication/finalization is still pending.
    pub pending: bool,
}

/// An applied conditional write or the verified state that superseded its expectation.
#[derive(Debug)]
pub enum Replaced {
    /// Accepted receipt and the requested hashes still missing at the node.
    Applied {
        /// Exact verified ownership receipt for the accepted request.
        state: State,
        /// Requested hashes that still need upload.
        missing: Vec<Vec<u8>>,
    },
    /// No list was replaced; this is the node's current verified state.
    Conflict(State),
}

impl State {
    /// Media revision, independent of the public profile version.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.wire.version
    }

    /// Exact signed ownership record, absent for a new or attachment-only owner.
    #[must_use]
    pub fn record(&self) -> Option<&SignedRecord> {
        self.wire.record.as_ref()
    }

    /// The verified current hash list; empty for an owner without a signed keep.
    #[must_use]
    pub fn hashes(&self) -> &[Vec<u8>] {
        self.projection
            .as_ref()
            .map_or(&[], |keep| keep.hashes.as_slice())
    }

    /// Target profile digest, absent on legacy ownership records.
    #[must_use]
    pub fn intent_hash(&self) -> Option<[u8; 32]> {
        self.projection
            .as_ref()?
            .intent_hash
            .as_slice()
            .try_into()
            .ok()
    }

    /// Whether this record describes a prepared publication rather than finalized ownership.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.projection.as_ref().is_some_and(|keep| keep.pending)
    }

    fn ensure_owner(&self, owner: &Identity) -> Result<(), Error> {
        if self.owner_key != owner.public_key() {
            return Err(Error::Input("media state belongs to another owner"));
        }
        Ok(())
    }

    fn condition(&self) -> proto::MediaKeepCondition {
        proto::MediaKeepCondition {
            owner_present: self.wire.owner_present,
            version: self.wire.version,
            record_hash: self
                .record()
                .map_or_else(Vec::new, |record| sha256(&record.encode_to_vec()).to_vec()),
        }
    }

    fn verify(owner: &Identity, wire: proto::MediaKeepState) -> Result<Self, Error> {
        if wire.version > i64::MAX.cast_unsigned()
            || (!wire.owner_present && (wire.version != 0 || wire.record.is_some()))
        {
            return Err(Error::Reply("invalid media ownership state"));
        }
        let projection = wire
            .record
            .as_ref()
            .map(|record| verify_record(owner, record, wire.version))
            .transpose()?;
        if wire.record.is_none() && wire.version != 0 {
            return Err(Error::Reply("unsigned media ownership cannot be verified"));
        }
        Ok(Self {
            owner_key: owner.public_key(),
            wire,
            projection,
        })
    }
}

fn verify_record(
    owner: &Identity,
    record: &SignedRecord,
    version: u64,
) -> Result<proto::MediaKeepIntent, Error> {
    if record.encoded_len() > MAX_RECORD || record.public_key.as_slice() != owner.public_key() {
        return Err(Error::Reply(
            "media ownership belongs to another owner or is too large",
        ));
    }
    let keep = proto::MediaKeepIntent::decode(record.payload.as_slice())
        .map_err(|_| Error::Reply("invalid media keep payload"))?;
    let domain = if keep.expected.is_some() {
        Domain::MediaConditionalKeep
    } else {
        Domain::MediaKeep
    };
    record
        .verify(domain)
        .map_err(|_| Error::Reply("invalid media ownership signature"))?;
    if keep.version != version || version == 0 || !valid_hashes(&keep.hashes) {
        return Err(Error::Reply("media ownership revision or hashes differ"));
    }
    match &keep.expected {
        Some(expected)
            if valid_condition(expected)
                && keep.intent_hash.len() == 32
                && keep.requester.parse::<libp2p::PeerId>().is_ok()
                && expected.version.checked_add(1) == Some(version) => {}
        None if keep.intent_hash.is_empty() && !keep.pending && keep.requester.is_empty() => {}
        _ => return Err(Error::Reply("invalid signed media intent")),
    }
    Ok(keep)
}

fn valid_hashes(hashes: &[Vec<u8>]) -> bool {
    hashes.len() <= MAX_KEEP && hashes.iter().all(|hash| hash.len() == 32)
}

fn valid_condition(condition: &proto::MediaKeepCondition) -> bool {
    condition.version <= i64::MAX.cast_unsigned()
        && matches!(condition.record_hash.len(), 0 | 32)
        && (condition.owner_present || (condition.version == 0 && condition.record_hash.is_empty()))
        && (condition.record_hash.is_empty() || condition.version > 0)
}

fn verify_missing(missing: &[Vec<u8>], requested: &[Vec<u8>]) -> Result<(), Error> {
    if missing.len() > requested.len()
        || missing.iter().enumerate().any(|(i, hash)| {
            hash.len() != 32 || !requested.contains(hash) || missing[..i].contains(hash)
        })
    {
        return Err(Error::Reply(
            "media reply names unrequested or duplicate missing hashes",
        ));
    }
    Ok(())
}

fn parse_replaced(
    owner: &Identity,
    record: &SignedRecord,
    requested: &[Vec<u8>],
    response: proto::MediaResponse,
) -> Result<Replaced, Error> {
    match Status::try_from(response.status) {
        Ok(Status::Ok) if response.keep_applied => {
            let state = State::verify(
                owner,
                response
                    .keep_state
                    .map(|state| *state)
                    .ok_or(Error::Reply("missing keep receipt"))?,
            )?;
            if state.record() != Some(record) {
                return Err(Error::Reply("media keep receipt differs from request"));
            }
            verify_missing(&response.missing, requested)?;
            Ok(Replaced::Applied {
                state,
                missing: response.missing,
            })
        }
        Ok(Status::Stale) if !response.keep_applied && response.missing.is_empty() => {
            let state = State::verify(
                owner,
                response
                    .keep_state
                    .map(|state| *state)
                    .ok_or(Error::Reply("missing media conflict state"))?,
            )?;
            Ok(Replaced::Conflict(state))
        }
        Ok(Status::Ok | Status::Stale) => Err(Error::Reply("invalid conditional keep outcome")),
        _ => {
            ok(response.status)?;
            Err(Error::Reply("invalid media keep status"))
        }
    }
}

impl Node {
    /// Read this owner's media state without changing its list or refreshing retention.
    ///
    /// # Errors
    /// Returns request/status errors or malformed, unsigned or wrongly signed reply errors.
    /// Unsupported peers are never retried through the weaker legacy keep operation.
    pub async fn media_keep_state(&mut self, owner: &Identity) -> Result<State, Error> {
        let query = proto::MediaKeepIntent {
            time: now(),
            requester: self.swarm.local_peer_id().to_base58(),
            ..Default::default()
        };
        let record = owner.sign(Domain::MediaConditionalKeep, query.encode_to_vec());
        let response = self
            .media(media_request::Request::ConditionalKeep(record))
            .await?;
        ok(response.status)?;
        if response.keep_applied || !response.missing.is_empty() {
            return Err(Error::Reply("media state query returned a write receipt"));
        }
        State::verify(
            owner,
            response
                .keep_state
                .map(|state| *state)
                .ok_or(Error::Reply("missing media ownership state"))?,
        )
    }

    /// Sign a conditional next-revision keep, binding the exact expectation and profile intent.
    ///
    /// # Errors
    /// Returns invalid hash/counter input, request/status or unverified/mismatched receipt errors.
    /// A conflict is returned separately and never causes an unconditional retry.
    pub async fn replace_media_keep(
        &mut self,
        owner: &Identity,
        expected: &State,
        hashes: Vec<Vec<u8>>,
        intent: Intent,
    ) -> Result<Replaced, Error> {
        if !valid_hashes(&hashes) || expected.revision() >= i64::MAX.cast_unsigned() {
            return Err(Error::Input(
                "invalid media hashes or exhausted storage revision",
            ));
        }
        // States are identity-bound even when a caller retains one across an identity switch.
        expected.ensure_owner(owner)?;
        let keep = proto::MediaKeepIntent {
            version: expected.revision() + 1,
            hashes,
            time: now(),
            expected: Some(expected.condition()),
            intent_hash: intent.profile_hash.to_vec(),
            pending: intent.pending,
            requester: self.swarm.local_peer_id().to_base58(),
        };
        let record = owner.sign(Domain::MediaConditionalKeep, keep.encode_to_vec());
        let response = self
            .media(media_request::Request::ConditionalKeep(record.clone()))
            .await?;
        parse_replaced(owner, &record, &keep.hashes, response)
    }
}
