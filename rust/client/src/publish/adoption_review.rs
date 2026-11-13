//! Immutable owner-bound adoption comparison; selecting fields never writes or publishes.
use super::{adoption_fields::AdoptionField, newest, Failure, Identity, Node, Profile, Store};
use crate::store::ProfileAdoptionContext;
use std::collections::BTreeMap;

/// The owner-selected source for one differing editable field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdoptionSource {
    /// Keep the frozen local field value.
    Local,
    /// Keep the verified remote field value.
    Remote,
}

/// A malformed or incomplete manual field decision; no profile state has changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AdoptionSelectionError {
    /// Every differing field needs an explicit owner choice.
    #[error("choose a source for every differing profile field")]
    Incomplete,
    /// One field has more than one source decision.
    #[error("a profile field was selected more than once")]
    Duplicate,
    /// A field outside this comparison was submitted.
    #[error("the selected field is not offered by this review")]
    UnexpectedField,
}

/// A frozen local publication baseline and the verified ordinary profile from one remote read.
///
/// It owns no key, database transaction or identity lease. Confirmation must revalidate context;
/// field selection alone does not authorize writes, complete the link or clear its pending row.
pub struct AdoptionReview {
    pub(super) context: ProfileAdoptionContext,
    pub(super) remote: Profile,
}

impl AdoptionReview {
    /// The exact local snapshot, before the remote request.
    #[must_use]
    pub fn local_profile(&self) -> &Profile {
        self.context.publication.draft()
    }

    /// The exact decoded ordinary remote readback, never overlaid with local choices.
    #[must_use]
    pub fn remote_profile(&self) -> &Profile {
        &self.remote
    }

    /// Exact pending private-state snapshot captured with this comparison.
    /// Adapters use it to refuse completion of a replacement link; it authorizes no writes.
    #[must_use]
    pub fn pending_link_state(&self) -> &[u8] {
        self.context.pending_link_state()
    }

    /// Retain verified historical membership when actual recovery starts. This does not select
    /// fields, authorize relisting, or modify the prepared comparison. Preparation stays read-only.
    /// Call before resumption's fresh network read, while holding the original session admission.
    ///
    /// # Errors
    /// Refuses another owner, database or pending operation and propagates storage/session errors.
    pub fn remember_device_membership(
        &self,
        session: &crate::session::Session,
        identity: &Identity,
    ) -> Result<(), crate::session::Error> {
        let certificate = session.certificate()?;
        if certificate.public_key != identity.public_key() {
            return Err(crate::store::Error::WrongIdentity.into());
        }
        let listed = (!self.remote.deleted && self.remote.devices.contains(&certificate))
            .then_some(self.remote.version);
        session
            .store()
            .remember_reviewed_membership(identity, &self.context, listed)?;
        Ok(())
    }

    /// Visibility captured with the local baseline.
    #[must_use]
    pub fn hidden(&self) -> bool {
        self.context.publication.hidden()
    }

    /// Check local review context without holding a lease across the owner's decision.
    /// This is a preflight: confirmation must repeat the check under its completion writer
    /// and separately verify the remote readback.
    ///
    /// # Errors
    /// Returns wrong owner or invalid/unreadable local state.
    pub fn is_current(
        &self,
        store: &Store,
        identity: &Identity,
    ) -> Result<bool, crate::store::Error> {
        store.profile_adoption_context_current(identity, &self.context)
    }

    /// Differing editable fields in the shared stable presentation order.
    #[must_use]
    pub fn differences(&self) -> Vec<AdoptionField> {
        AdoptionField::ALL
            .iter()
            .copied()
            .filter(|field| field.differs(self.local_profile(), &self.remote))
            .collect()
    }

    /// Build a preview from explicit decisions, preserving remote device authorization.
    /// Stronger local proof survives; version/deleted state remain the verified remote values.
    ///
    /// # Errors
    /// Rejects duplicate, unoffered or missing field choices without modifying either snapshot.
    pub fn select_fields(
        &self,
        choices: &[(AdoptionField, AdoptionSource)],
    ) -> Result<Profile, AdoptionSelectionError> {
        let offered = self.differences();
        let mut selected = BTreeMap::new();
        for &(field, source) in choices {
            if selected.insert(field, source).is_some() {
                return Err(AdoptionSelectionError::Duplicate);
            }
            if !offered.contains(&field) {
                return Err(AdoptionSelectionError::UnexpectedField);
            }
        }
        if selected.len() != offered.len() {
            return Err(AdoptionSelectionError::Incomplete);
        }
        let local = self.local_profile();
        let mut preview = self.remote.clone();
        for (field, source) in selected {
            if source == AdoptionSource::Local {
                field.copy(local, &mut preview);
            }
        }
        if local.pow_bits > preview.pow_bits {
            (preview.pow_nonce, preview.pow_bits) = (local.pow_nonce, local.pow_bits);
        }
        Ok(preview)
    }
}

/// Freeze the local baseline before reading the linked account's verified remote profile.
/// No draft, visibility, credentials, receipt or pending-link row is changed.
///
/// # Errors
/// Returns missing pending link, wrong owner, unavailable/deleted profile or storage/network errors.
pub async fn review_adoption(
    store: &Store,
    node: &mut Node,
    identity: &Identity,
) -> Result<AdoptionReview, Failure> {
    let context = store
        .prepare_profile_adoption_context(identity)?
        .ok_or(super::Error::Input("no pending linked-device recovery"))?;
    let remote = newest(node, identity)
        .await?
        .ok_or(super::Error::Reply("the account has no profile"))?;
    if remote.deleted {
        return Err(Failure::AdoptionReviewRequired);
    }
    Ok(AdoptionReview { context, remote })
}

#[cfg(test)]
mod tests;
