//! Upload and send an encrypted media descriptor with a message-scoped ID and HLC.

use super::{Body, Error, Node, Session, Submission};
use crate::attachment::Attachment;
use crate::chat::AttachmentMessage;
use crate::scores::Scores;
use crate::selection::Operation;
use dyapp_identity::Identity;

pub(super) async fn upload(
    nodes: &mut [Node],
    identity: &Identity,
    attachment: &Attachment,
    blob: &[u8],
    scores: &mut Scores,
) -> Result<(), Error> {
    let mut last = crate::Error::Request("no node".into());
    let mut uploaded = false;
    scores.register_nodes(nodes);
    let peers: Vec<_> = nodes.iter().map(Node::peer).collect();
    let order = scores.ranked(&peers, Operation::Media);
    for index in order {
        let Some(node) = nodes.get_mut(index) else {
            continue;
        };
        let peer = node.peer();
        let started = std::time::Instant::now();
        let upload = async {
            if !node
                .attach_media(identity, attachment, crate::now())
                .await?
                .is_empty()
            {
                node.put_media(&identity.peer_id(), blob.to_vec()).await?;
            }
            Ok(())
        };
        match upload.await {
            Ok(()) => {
                scores.success(peer, Operation::Media, started.elapsed());
                uploaded = true;
                break;
            }
            Err(error) => {
                scores.fault(peer, crate::now(), Operation::Media);
                last = error;
            }
        }
    }
    if !uploaded {
        return Err(last.into());
    }
    Ok(())
}

impl Session {
    /// Upload the `blob` from [`crate::attachment::seal`], signed by the owner `identity`, to the
    /// first of `nodes` that takes it, then send its [`Attachment`], with any preview, MIME type
    /// and dimensions set, like a text. Nothing is staged when no node takes the blob.
    ///
    /// # Errors
    ///
    /// Returns an error if attachment validation, upload, randomness, clock access,
    /// or message staging/delivery fails. A blob may already be uploaded, and
    /// staged writes remain durable when delivery fails.
    pub async fn send_attachment(
        &mut self,
        nodes: &mut [Node],
        identity: &Identity,
        group_id: &[u8],
        attachment: &Attachment,
        blob: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<[u8; 16], Error> {
        self.submit_attachment(nodes, identity, group_id, attachment, blob, seal_key)
            .await?
            .into_accepted()
    }

    /// Upload an attachment before committing its message; retain the ID on delivery failure.
    ///
    /// # Errors
    /// Returns validation, upload or staging errors before successful message commitment.
    pub async fn submit_attachment(
        &mut self,
        nodes: &mut [Node],
        identity: &Identity,
        group_id: &[u8],
        attachment: &Attachment,
        blob: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<Submission, Error> {
        attachment.check()?;
        {
            let mut scores = self.scores.lock().await;
            upload(nodes, identity, attachment, blob, &mut scores).await?;
        }
        self.submit_uploaded_attachment(nodes, group_id, attachment, false, seal_key)
            .await
    }

    // The production caller reaches this phase only after validating and uploading the blob.
    pub(super) async fn submit_uploaded_attachment(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        attachment: &Attachment,
        forwarded: bool,
        seal_key: &[u8; 32],
    ) -> Result<Submission, Error> {
        let mut id = [0; 16];
        getrandom::fill(&mut id)
            .map_err(|_| Error::Client(crate::Error::Reply("no message randomness")))?;
        let hlc = self.store.draft_chat_text(crate::now_ms())?;
        let body = Body::Attachment(AttachmentMessage {
            id,
            hlc,
            attachment: attachment.clone(),
        });
        let body = if forwarded {
            Body::Forwarded(Box::new(body))
        } else {
            body
        };
        let writes = self.stage_message(group_id, &body, seal_key)?;
        Ok(self.submit_staged(nodes, id, writes, seal_key).await)
    }

    /// Submit through the canonical open pair group, retaining the committed message ID.
    ///
    /// # Errors
    /// Returns missing-group, blocked/closed conversation, validation, upload or staging errors.
    pub async fn submit_attachment_to(
        &mut self,
        nodes: &mut [Node],
        identity: &Identity,
        peer_id: &str,
        attachment: &Attachment,
        blob: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<Submission, Error> {
        let group_id = self.pair_group(peer_id).ok_or(Error::UnknownGroup)?;
        if self.store.blocked(peer_id)? {
            return Err(crate::store::Error::BlockedPeer.into());
        }
        if self.store.conversation(peer_id)? != crate::store::Conversation::Open {
            return Err(Error::ConversationNotOpen);
        }
        self.submit_attachment(nodes, identity, &group_id, attachment, blob, seal_key)
            .await
    }

    /// Forward a current photo into another open chat under freshly generated media credentials.
    ///
    /// # Errors
    /// Returns an error for a stale source, invalid original bytes, ineligible destination,
    /// or a failure to encrypt, upload or stage the forwarded message.
    pub async fn submit_forwarded_photo(
        &mut self,
        nodes: &mut [Node],
        identity: &Identity,
        source: &crate::attachment::original::Snapshot,
        original: &[u8],
        target_peer: &str,
        seal_key: &[u8; 32],
    ) -> Result<Submission, Error> {
        if source.peer() == target_peer {
            return Err(Error::ConversationNotOpen);
        }
        let group_id = self.pair_group(target_peer).ok_or(Error::UnknownGroup)?;
        if self.store.blocked(target_peer)? {
            return Err(crate::store::Error::BlockedPeer.into());
        }
        if self.store.conversation(target_peer)? != crate::store::Conversation::Open {
            return Err(Error::ConversationNotOpen);
        }
        source.current(&self.store)?;
        source.validate(original)?;

        let previous = source.attachment();
        let (mut attachment, blob) = crate::attachment::seal(original)?;
        attachment.preview.clone_from(&previous.preview);
        attachment.mime.clone_from(&previous.mime);
        attachment.width = previous.width;
        attachment.height = previous.height;
        attachment.check()?;
        {
            let mut scores = self.scores.lock().await;
            upload(nodes, identity, &attachment, &blob, &mut scores).await?;
        }
        source.current(&self.store)?;
        self.submit_uploaded_attachment(nodes, &group_id, &attachment, true, seal_key)
            .await
    }

    /// [`Session::send_attachment`] into the pair's canonical joined group, which must be an open
    /// conversation: an attachment never opens a message request.
    ///
    /// # Errors
    ///
    /// Returns an error if the pair has no group, the peer is blocked, the chat is
    /// not open, storage checks fail, or [`Self::send_attachment`] fails.
    pub async fn send_attachment_to(
        &mut self,
        nodes: &mut [Node],
        identity: &Identity,
        peer_id: &str,
        attachment: &Attachment,
        blob: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<[u8; 16], Error> {
        self.submit_attachment_to(nodes, identity, peer_id, attachment, blob, seal_key)
            .await?
            .into_accepted()
    }
}
