//! Upload and send an encrypted media descriptor with a message-scoped ID and HLC.

use super::{Body, Error, Node, Session};
use crate::attachment::Attachment;
use crate::chat::AttachmentMessage;
use dyapp_identity::Identity;

impl Session {
    /// Upload the `blob` from [`crate::attachment::seal`], signed by the owner `identity`, to the
    /// first of `nodes` that takes it, then send its [`Attachment`], with any preview, MIME type
    /// and dimensions set, like a text. Nothing is staged when no node takes the blob.
    pub async fn send_attachment(
        &mut self,
        nodes: &mut [Node],
        identity: &Identity,
        group_id: &[u8],
        attachment: &Attachment,
        blob: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<[u8; 16], Error> {
        attachment.check()?;
        let mut last = crate::Error::Request("no node".into());
        let mut uploaded = false;
        for node in nodes.iter_mut() {
            let upload = async {
                if !node.attach_media(identity, attachment).await?.is_empty() {
                    node.put_media(&identity.peer_id(), blob.to_vec()).await?;
                }
                Ok(())
            };
            match upload.await {
                Ok(()) => {
                    uploaded = true;
                    break;
                }
                Err(error) => last = error,
            }
        }
        if !uploaded {
            return Err(last.into());
        }
        let mut id = [0; 16];
        getrandom::fill(&mut id)
            .map_err(|_| Error::Client(crate::Error::Reply("no message randomness")))?;
        let hlc = self.store.draft_chat_text(crate::now() * 1000)?;
        self.send_body(
            nodes,
            group_id,
            &Body::Attachment(AttachmentMessage {
                id,
                hlc,
                attachment: attachment.clone(),
            }),
            seal_key,
        )
        .await?;
        Ok(id)
    }
}
