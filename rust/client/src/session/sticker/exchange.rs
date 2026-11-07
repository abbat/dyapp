//! Pack controls carry local selection/job context into the same MLS/outbox transaction.
use super::{Body, Error, Node, PreparedEnvelope, Session};
use crate::chat::{self, pack_control::PackAnswer};
use crate::store::{
    messages::MessageKey,
    sticker_packs::exchange::{response::PackResponseJob, PendingPack},
    Store,
};
use dyapp_p2p_net::proto::EnvelopeClass;

impl Session {
    fn stage_pack_control<R, F>(
        &mut self,
        group_id: &[u8],
        body: &Body,
        seal_key: &[u8; 32],
        project: F,
    ) -> Result<(R, Vec<PreparedEnvelope>), Error>
    where
        F: FnOnce(&mut Store, &[u8], &[PreparedEnvelope]) -> Result<R, crate::store::Error>,
    {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let group = self
            .groups
            .iter_mut()
            .find(|group| group.id() == group_id)
            .ok_or(Error::UnknownGroup)?;
        let staged = (|| {
            let deliveries = chat::message(group, device, body)?;
            let writes =
                super::super::prepare(&self.store, deliveries, EnvelopeClass::Message, seal_key)?;
            let sealed = device.seal(seal_key)?;
            let result = project(&mut self.store, &sealed, &writes)?;
            Ok((result, writes))
        })();
        if staged.is_err() {
            self.device = None;
            self.groups.clear();
        }
        staged
    }

    /// Derive a request from the selected received sticker and atomically stage its intent and writes.
    pub fn stage_sticker_pack_request(
        &mut self,
        peer: &str,
        selected: &MessageKey,
        seal_key: &[u8; 32],
    ) -> Result<(PendingPack, Vec<PreparedEnvelope>), Error> {
        let group = self.pair_group(peer).ok_or(Error::UnknownGroup)?;
        let request = self.store.sticker_pack_request(&group, selected)?;
        let body = Body::PackRequest(request.clone());
        self.stage_pack_control(&group, &body, seal_key, |store, sealed, writes| {
            store.stage_sticker_pack_request(sealed, writes, &group, selected, &request)
        })
    }

    /// Send only after the selected request and exact encrypted writes are durable.
    pub async fn send_sticker_pack_request(
        &mut self,
        nodes: &mut [Node],
        peer: &str,
        selected: &MessageKey,
        seal_key: &[u8; 32],
    ) -> Result<PendingPack, Error> {
        let (pending, writes) = self.stage_sticker_pack_request(peer, selected, seal_key)?;
        self.send_staged(nodes, writes, seal_key).await?;
        Ok(pending)
    }

    /// Stage a validated descriptor for the current response job, advancing cooldown with the outbox.
    pub fn stage_sticker_pack_response(
        &mut self,
        job: &PackResponseJob,
        answer: &PackAnswer,
        seal_key: &[u8; 32],
    ) -> Result<Vec<PreparedEnvelope>, Error> {
        answer.encode()?;
        let body = Body::PackAnswer(Box::new(answer.clone()));
        let (_, writes) =
            self.stage_pack_control(&job.group, &body, seal_key, |store, sealed, writes| {
                store.stage_sticker_pack_response(sealed, writes, job, answer, crate::now() * 1000)
            })?;
        Ok(writes)
    }

    /// Deliver a response only after exact writes and its daily cooldown commit.
    pub async fn send_sticker_pack_response(
        &mut self,
        nodes: &mut [Node],
        job: &PackResponseJob,
        answer: &PackAnswer,
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let writes = self.stage_sticker_pack_response(job, answer, seal_key)?;
        self.send_staged(nodes, writes, seal_key).await
    }
}

mod retry;
#[cfg(test)]
mod tests;
