//! Thin explicit-opening bridge; privacy and lifetime policy remain in the shared core.
use super::{failed, listed_own, ChatSession, ChatSubmission};
use crate::profile_network::{active_network, connect, on_network_stack};
use crate::{CoreError, ProfileSearcher};
use dyapp_client::interactions::{profile_view, Kind};
use dyapp_client::session::{Session, Submission};
use std::sync::Arc;

/// One explicit profile opening. Retire on disposal, not on a background profile read.
#[derive(Debug, uniffi::Object)]
pub struct ProfileViewOpening {
    opening: profile_view::Opening,
}

#[uniffi::export]
impl ProfileViewOpening {
    /// Prevent a later worker from admitting this opening; admitted work keeps its original target.
    pub fn retire(&self) {
        self.opening.retire();
    }
}

#[uniffi::export]
impl ProfileSearcher {
    /// Capture an explicit opening through the already-held UI search context, without storage IO.
    /// The supplied owner is only a binding hint; submission compares it to the actual Session owner.
    ///
    /// # Errors
    /// Returns a poisoned context lock without reading profiles, preferences or active settings.
    pub fn profile_view_opening(
        &self,
        owner_peer: String,
        peer_id: String,
    ) -> Result<Arc<ProfileViewOpening>, CoreError> {
        let context = self.cache.lock().map_err(failed)?;
        Ok(Arc::new(ProfileViewOpening {
            opening: profile_view::Opening::new(&context, owner_peer, peer_id),
        }))
    }
}

#[uniffi::export]
impl ChatSession {
    /// Capture the original target and network generation without fetching profiles or reading policy.
    /// Call for an explicit opening only, before scheduling its submission.
    ///
    /// # Errors
    /// Returns a poisoned contact-context lock without creating a disclosure.
    pub fn profile_view_opening(
        &self,
        peer_id: String,
    ) -> Result<Arc<ProfileViewOpening>, CoreError> {
        let context = self.contact_context.lock().map_err(failed)?;
        Ok(Arc::new(ProfileViewOpening {
            opening: profile_view::Opening::new(&context, self.key.identity.peer_id(), peer_id),
        }))
    }

    /// Submit the original opening once. No-event, committed delivery and pre-commit refusal differ.
    /// Disabled, duplicate and retired openings avoid network lookup entirely.
    ///
    /// # Errors
    /// Returns context, privacy, publication, network, peer or staging failures before commitment.
    /// After commitment the exact persisted event reports Accepted or Queued independently of UI.
    pub fn submit_profile_view(
        &self,
        opening: Arc<ProfileViewOpening>,
    ) -> Result<Option<ChatSubmission>, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        let session: &mut Session = &mut guard;
        let network = {
            let context = self.contact_context.lock().map_err(failed)?;
            if !opening
                .opening
                .admit_at(&context, session.store(), dyapp_client::clock::now_ms())
                .map_err(failed)?
            {
                return Ok(None);
            }
            // Activation cannot replace settings between admission and this immutable snapshot.
            active_network(&self.dir)?
        };
        let own = listed_own(session, &self.key.identity)?;
        on_network_stack(move || {
            self.runtime.block_on(async move {
                let mut node = connect(&network).await?;
                let peer = node
                    .get_profile(opening.opening.peer())
                    .await
                    .map_err(failed)?;
                let submission = session
                    .submit_interaction_with(
                        std::slice::from_mut(&mut node),
                        &[own, peer],
                        Kind::View,
                        &self.seal_key,
                    )
                    .await
                    .map_err(failed)?;
                Ok(submission.map(Submission::from).map(ChatSubmission::from))
            })
        })
    }
}
