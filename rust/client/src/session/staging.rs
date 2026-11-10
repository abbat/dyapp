//! Message preflight and persistence dispatch around the parent's MLS commit boundary.

use super::{Error, Session};
use crate::chat::Body;
use crate::interactions::Interaction;
use crate::store::messages::VerifiedChatSender;
use crate::store::Store;
use crate::PreparedEnvelope;
use dyapp_messaging::mls::Device;

pub(super) struct Context {
    interaction: Option<(String, Vec<u8>, Interaction)>,
    presence: Option<(String, u64)>,
    read: Option<(String, [u8; 16])>,
}

pub(super) fn context(
    session: &Session,
    group_id: &[u8],
    body: &Body,
    device: &Device,
) -> Result<Context, Error> {
    let interaction = match body {
        Body::Interaction(event) => Some((
            session.partner(group_id).ok_or(Error::UnknownGroup)?,
            device.device_key()?,
            *event,
        )),
        _ => None,
    };
    let presence = match body {
        Body::Presence { hlc } => {
            Some((session.partner(group_id).ok_or(Error::UnknownGroup)?, *hlc))
        }
        _ => None,
    };
    let read = match body {
        Body::Read { up_to } => Some((
            session.partner(group_id).ok_or(Error::UnknownGroup)?,
            *up_to,
        )),
        _ => None,
    };
    Ok(Context {
        interaction,
        presence,
        read,
    })
}

pub(super) fn persist(
    store: &mut Store,
    device: &Device,
    group_id: &[u8],
    body: &Body,
    context: Context,
    sealed: &[u8],
    writes: &[PreparedEnvelope],
) -> Result<(), Error> {
    if let Some((target, source_device, event)) = context.interaction {
        store.stage_interaction(&target, &source_device, event, sealed, writes)?;
    } else if let Some((target, hlc)) = &context.presence {
        store.stage_presence(target, *hlc, sealed, writes)?;
    } else if let Some((peer, up_to)) = &context.read {
        store.stage_read(group_id, peer, *up_to, sealed, writes)?;
    } else if matches!(body, Body::Receipt(_)) {
        store.stage_receipt(sealed, writes)?;
    } else if let Body::PeerChange { peer, hlc, change } = body {
        let actor = device.device_key()?;
        store.stage_peer_change(peer, change, *hlc, sealed, writes, &actor)?;
    } else if body.attachment_message().is_some()
        || matches!(
            body,
            Body::Text { .. }
                | Body::Forwarded(_)
                | Body::Sticker(_)
                | Body::Edit { .. }
                | Body::Delete { .. }
        )
    {
        let author = store.owner_peer_id().to_owned();
        let source_device = device.device_key()?;
        let sender = VerifiedChatSender {
            group: group_id,
            author: &author,
            device: &source_device,
        };
        store.stage_chat_outbound(sealed, writes, sender, body, crate::now() * 1000)?;
    } else {
        store.stage_outbound(sealed, writes)?;
    }
    Ok(())
}
