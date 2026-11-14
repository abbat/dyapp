//! Bounded core pack work completes after native mailbox application, without visible controls.
use super::{apply_mailbox, failed, ChatSession};
use crate::CoreError;
use dyapp_client::{session::Session, Node};
use dyapp_identity::Identity;
use dyapp_profile::VerifiedProfile;

pub(super) async fn run(
    session: &mut Session,
    nodes: &mut [Node],
    identity: &Identity,
    seal: &[u8; 32],
) -> Result<u32, CoreError> {
    // The native receive call retains identity/session ownership through blocking completion.
    session
        .respond_sticker_pack_requests(nodes, identity, seal, || false)
        .await
        .map_err(failed)?;
    let saved = session
        .download_sticker_packs(nodes, seal, || false)
        .await
        .map_err(failed)?;
    u32::try_from(saved).map_err(failed)
}

impl ChatSession {
    pub(super) async fn receive_mailbox(
        &self,
        session: &mut Session,
        node: &mut Node,
        own: Option<VerifiedProfile>,
    ) -> Result<u32, CoreError> {
        let changed = apply_mailbox(
            session,
            node,
            own,
            &self.device_key.identity,
            &self.seal_key,
        )
        .await?;
        let saved = run(
            session,
            std::slice::from_mut(node),
            &self.key.identity,
            &self.seal_key,
        )
        .await?;
        changed
            .checked_add(saved)
            .ok_or_else(|| failed("changeCountOverflow"))
    }
}
