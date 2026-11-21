//! Mail progress and optional core-admitted notification content share one native session owner.
use super::{failed, refill_if_due, ChatSession};
use crate::profile_network::{active_network, connect, on_network_stack};
use crate::CoreError;
use dyapp_client::{
    contact_verification::Generation, mail_notices, session::Session, store::Store, Node,
};
use dyapp_identity::network::Network;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Safe core-admitted content; no sender means the localized fixed new-message notice.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatMailNotice {
    /// Allowed sender selector for local presentation, absent for likes/requests/fixed mode.
    pub peer_id: Option<String>,
    /// Plain bounded text, already including the trusted local sticker label when applicable.
    pub text: Option<String>,
}

/// Completed mail work and an optional notification, independent of OS posting permissions/focus.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatMailPass {
    /// Chat-event and saved-pack changes, as returned by receive.
    pub changed: u32,
    /// None for no eligible increase, disabled notices or failed notice snapshot/preferences.
    pub notice: Option<ChatMailNotice>,
}

pub(super) fn notice_after(
    before: Option<mail_notices::Waiting>,
    store: &Store,
    sticker_label: &str,
) -> Option<ChatMailNotice> {
    let before = before?;
    let after = mail_notices::capture(store).ok()?;
    let selected = mail_notices::decide(
        &before,
        &after,
        store.notifications().ok()?,
        store.notice_content().ok()?,
    )
    .ok()??;
    let text = selected.preview.plain_text(sticker_label);
    Some(ChatMailNotice {
        peer_id: selected.peer_id,
        text: (!text.is_empty()).then_some(text),
    })
}

#[uniffi::export]
impl ChatSession {
    /// Receive mail and project core-admitted notification content under the same session/identity
    /// lifetime. The sticker label is trusted local UI text, not peer content. Call off the UI thread.
    /// Notification failures stay quiet and never prevent mailbox work; OS posting remains local.
    ///
    /// # Errors
    /// Returns the same mail/network/storage errors as receive, not notification projection errors.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn receive_with_notice(&self, sticker_label: String) -> Result<ChatMailPass, CoreError> {
        self.receive_pass(Some(&sticker_label))
    }
}

impl ChatSession {
    pub(super) fn receive_pass(
        &self,
        sticker_label: Option<&str>,
    ) -> Result<ChatMailPass, CoreError> {
        let mut guard = self.session.lock().map_err(failed)?;
        let session: &mut Session = &mut guard;
        let before = sticker_label.and_then(|_| mail_notices::capture(session.store()).ok());
        let generation = self.contact_context.lock().map_err(failed)?.generation();
        let network = active_network(&self.dir)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(failed)?;
        // The guard stays on this thread and lends the session until actual network completion.
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                let changed = self
                    .receive_connected(session, &network, &mut node, &generation, now)
                    .await?;
                let notice =
                    sticker_label.and_then(|label| notice_after(before, session.store(), label));
                Ok(ChatMailPass { changed, notice })
            })
        })
    }

    async fn receive_connected(
        &self,
        session: &mut Session,
        network: &Network,
        node: &mut Node,
        generation: &Generation,
        now: Duration,
    ) -> Result<u32, CoreError> {
        let fresh_owner = self
            .receive_owner_profile(session, node, generation, now.as_secs())
            .await?;
        let protocols =
            std::sync::Arc::new(dyapp_p2p_net::Protocols::new(&network.name).map_err(failed)?);
        let mailbox = dyapp_client::mailbox(&self.device_key.identity.public_key());
        let mut replicas = node.replicas(&mailbox, protocols).await;
        let now_ms = i64::try_from(now.as_millis()).map_err(failed)?;
        session
            .retry_due(replicas.nodes_mut(), now_ms, &self.seal_key)
            .await
            .map_err(failed)?;
        let changed = self
            .receive_mailbox(
                session,
                node,
                replicas.nodes_mut(),
                generation,
                now.as_secs(),
            )
            .await?;
        let held = self
            .receive_contact_profiles(session, node, generation, now.as_secs())
            .await?;
        let profiles = held.all().to_vec();
        let fresh = fresh_owner.then_some(held);
        let checked = self
            .receive_cross_check(session, &mut replicas, &profiles, now.as_secs())
            .await?;
        let changed = changed
            .checked_add(checked)
            .ok_or_else(|| failed("changeCountOverflow"))?;
        self.maintain_received_own(session, replicas.nodes_mut(), fresh.as_ref(), now.as_secs())
            .await;
        self.maintain_receive(session, node, replicas.nodes_mut(), now)
            .await?;
        Ok(changed)
    }

    async fn receive_cross_check(
        &self,
        session: &mut Session,
        replicas: &mut dyapp_client::ReplicaSet,
        profiles: &[dyapp_profile::VerifiedProfile],
        now: u64,
    ) -> Result<u32, CoreError> {
        let events = session
            .cross_check_due(
                replicas,
                &self.device_key.identity,
                profiles,
                &self.seal_key,
                now,
            )
            .await
            .map_err(failed)?;
        let changed = events
            .iter()
            .filter(|event| {
                !matches!(
                    event,
                    dyapp_client::chat::Event::Gap(_)
                        | dyapp_client::chat::Event::Refused(_)
                        | dyapp_client::chat::Event::ResendRequired
                )
            })
            .count();
        u32::try_from(changed).map_err(failed)
    }

    async fn maintain_receive(
        &self,
        session: &mut Session,
        node: &mut Node,
        nodes: &mut [Node],
        now: Duration,
    ) -> Result<(), CoreError> {
        refill_if_due(session, nodes, now.as_secs(), &self.seal_key).await?;
        // Preserve established best-effort last-resort rotation and daily photo renewal.
        if matches!(session.store().last_resort(), Ok(Some(_))) {
            let _ = session
                .keep_last_resort(
                    node,
                    &self.device_key.identity,
                    now.as_secs(),
                    &self.seal_key,
                )
                .await;
        }
        let _ = session
            .refresh_profile_photos(node, &self.key.identity, now.as_secs())
            .await;
        Ok(())
    }
}
