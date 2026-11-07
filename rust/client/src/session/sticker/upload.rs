//! Saved-pack sends retain owner descriptors locally and expose only a stripped chat descriptor.
use super::{Error, Node, Session, Sticker};
use crate::attachment::{self, Attachment};
use crate::sticker_pack::Pack;
use crate::store::sticker_packs::uploads::UploadPlan;
use dyapp_identity::{sha256, Identity};

fn prepare(pack: &Pack, index: usize) -> Result<(Sticker, Attachment), crate::Error> {
    let entry = pack
        .stickers()
        .get(index)
        .ok_or(crate::Error::Reply("unknown sticker"))?;
    let bytes = pack
        .sticker_bytes(index)
        .ok_or(crate::Error::Reply("unknown sticker"))?;
    let frame = crate::sticker_webp::decode::first_frame(bytes).map_err(crate::Error::Reply)?;
    let (mut candidate, _) = attachment::seal(bytes)?;
    candidate.mime = "image/webp".into();
    candidate.width = frame.width;
    candidate.height = frame.height;
    let sticker = Sticker {
        id: [0; 16],
        hlc: 0,
        pack_id: *pack.id(),
        sha256: entry.sha256,
        emoji: entry.emoji.clone(),
        pack_name: pack.name().into(),
        attachment: None,
    };
    sticker.encode()?;
    Ok((sticker, candidate))
}

fn ciphertext(descriptor: &Attachment, bytes: &[u8]) -> Result<Vec<u8>, crate::Error> {
    // Never reuse an AES-GCM key/nonce sequence with different plaintext.
    if descriptor.size != bytes.len() as u64 || descriptor.plaintext_sha256 != sha256(bytes) {
        return Err(crate::Error::Reply("sticker plaintext mismatch"));
    }
    let blob = attachment::encrypt(&descriptor.key, bytes)?;
    if sha256(&blob) != descriptor.blob_sha256 {
        return Err(crate::Error::Reply("sticker ciphertext mismatch"));
    }
    Ok(blob)
}

impl Session {
    /// Upload/reuse one validated saved-pack entry, then stage its metadata through the outbox.
    /// Upload failure leaves a resumable reservation and does not stage a chat message.
    pub async fn send_pack_sticker_to(
        &mut self,
        nodes: &mut [Node],
        identity: &Identity,
        peer_id: &str,
        pack: &Pack,
        index: usize,
        seal_key: &[u8; 32],
    ) -> Result<[u8; 16], Error> {
        if identity.peer_id() != self.store.owner_peer_id() {
            return Err(crate::Error::Reply("sticker owner mismatch").into());
        }
        let group = self.pair_group(peer_id).ok_or(Error::UnknownGroup)?;
        if self.store.blocked(peer_id)? {
            return Err(crate::store::Error::BlockedPeer.into());
        }
        if self.store.conversation(peer_id)? != crate::store::Conversation::Open {
            return Err(Error::ConversationNotOpen);
        }
        let (mut sticker, candidate) = prepare(pack, index)?;
        let descriptor = match self.store.plan_sticker_upload(&candidate, crate::now())? {
            UploadPlan::Fallback => None,
            UploadPlan::Reuse(descriptor) => Some(descriptor),
            UploadPlan::Upload(pending) => {
                let bytes = pack
                    .sticker_bytes(index)
                    .ok_or(crate::Error::Reply("unknown sticker"))?;
                let blob = ciphertext(&pending.attachment, bytes)?;
                // Anchor local expiry before the node's signed attach timestamp, never after it.
                let started = crate::now();
                super::super::attachment::upload(nodes, identity, &pending.attachment, &blob)
                    .await?;
                self.store.finish_sticker_upload(&pending, started)?;
                Some(pending.attachment)
            }
        };
        sticker.attachment = descriptor.map(|mut descriptor| {
            descriptor.release_secret = [0; 32];
            descriptor
        });
        self.send_sticker(nodes, &group, &sticker, seal_key).await
    }
}

#[cfg(test)]
mod tests;
