//! Search withdrawals remain verifiable without becoming ordinary storage authorization.
use crate::{verify, verify_historical, Domain, Error, Profile, SignedRecord, VerifiedProfile};
use prost::Message as _;

impl Profile {
    /// A search-only removal marker; signing it never authorizes ordinary account deletion.
    #[must_use]
    pub fn search_tombstone(version: u64, pow_nonce: u64, pow_bits: u32) -> Self {
        Self {
            search_only: true,
            ..Self::tombstone(version, pow_nonce, pow_bits)
        }
    }
}

/// Verify search content, including purpose-separated removal markers.
/// Ordinary profile consumers must use `verify` instead.
///
/// # Errors
/// Returns size, signature, decoding and content errors, as for ordinary verification.
pub fn verify_search(record: &SignedRecord) -> Result<VerifiedProfile, Error> {
    match verify_historical(record) {
        Ok(profile) => return Ok(profile),
        Err(Error::Signature(_)) => {}
        Err(error) => return Err(error),
    }
    // Ordinary verification checked the byte limit; authenticate before protobuf allocation.
    let peer_id = record.verify(Domain::SearchWithdrawal)?;
    let profile = Profile::decode(record.payload.as_slice()).map_err(|_| Error::Decode)?;
    if profile.signature_domain() != Domain::SearchWithdrawal {
        return Err(dyapp_identity::Error::InvalidSignature.into());
    }
    profile.validate(&record.public_key)?;
    Ok(VerifiedProfile { peer_id, profile })
}

/// Verify ordinary write authorization, as for authoritative reads through [`verify`].
/// Version and previous-state checks still require the actual current record under the writer lock.
///
/// # Errors
/// Returns ordinary verification errors or insufficient deletion-purpose authorization.
pub fn verify_storage(record: &SignedRecord) -> Result<VerifiedProfile, Error> {
    verify(record)
}

impl VerifiedProfile {
    /// Check authority for installing an ordinary profile, independently of version admission.
    /// Historical unguarded tombstones remain readable but cannot authorize a new storage write.
    ///
    /// # Errors
    /// Returns `Error::Invalid` for search-only or legacy ambiguous deletion authority.
    pub fn check_storage_authorization(&self) -> Result<(), Error> {
        if self.profile.search_only || (self.profile.deleted && self.profile.previous.is_none()) {
            return Err(Error::Invalid(
                "profile deletion needs conditional owner authorization",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
