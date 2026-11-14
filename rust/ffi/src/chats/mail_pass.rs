//! Mail progress and optional core-admitted notification content share one native session owner.
use super::{failed, own_profile, refill_if_due, ChatSession};
use crate::profile_network::{active_network, connect, on_network_stack};
use crate::CoreError;
use dyapp_client::{mail_notices, session::Session, store::Store, Node};
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
        let own = own_profile(session.store(), &self.key.identity)?;
        let network = active_network(&self.dir)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(failed)?;
        let now_ms = i64::try_from(now.as_millis()).map_err(failed)?;
        // The guard stays on this thread and lends the session until actual network completion.
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                session
                    .retry_due(std::slice::from_mut(&mut node), now_ms, &self.seal_key)
                    .await
                    .map_err(failed)?;
                let changed = self.receive_mailbox(session, &mut node, own).await?;
                self.maintain_receive(session, &mut node, now).await?;
                let notice =
                    sticker_label.and_then(|label| notice_after(before, session.store(), label));
                Ok(ChatMailPass { changed, notice })
            })
        })
    }

    async fn maintain_receive(
        &self,
        session: &mut Session,
        node: &mut Node,
        now: Duration,
    ) -> Result<(), CoreError> {
        refill_if_due(session, node, now.as_secs(), &self.seal_key).await?;
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
        let _ = dyapp_client::publish::refresh_photos(
            session.store(),
            node,
            &self.key.identity,
            now.as_secs(),
        )
        .await;
        Ok(())
    }
}
