use super::{Error, Session};
use crate::chat;
use crate::selection::Operation;
use crate::Node;

impl Session {
    pub(super) async fn record_receipt(
        &mut self,
        nodes: &mut [Node],
        group: &[u8],
        sender: &str,
        device: &[u8],
        receipt: &chat::Receipt,
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let Some(group_state) = self.groups.iter().find(|known| known.id() == group) else {
            return Ok(());
        };
        let Ok(device_key) = <&[u8; 32]>::try_from(device) else {
            return Ok(());
        };
        if crate::mailbox(device_key) != receipt.mailbox.as_slice()
            || !group_state.members()?.iter().any(|member| member == sender)
        {
            return Ok(());
        }
        let mailbox = receipt.mailbox.as_slice().try_into().unwrap_or([0; 32]);
        let ids = receipt
            .ids
            .iter()
            .filter_map(|id| id.as_slice().try_into().ok())
            .collect::<Vec<_>>();
        let now = crate::now();
        for peer in self.store.receipted_in_group(&mailbox, group, &ids)?.1 {
            if let Ok(peer) = peer.parse() {
                self.scores
                    .lock()
                    .await
                    .fault(peer, now, Operation::MailboxWrite);
            }
        }
        for id in &receipt.refused {
            let Ok(id) = id.as_slice().try_into() else {
                continue;
            };
            self.recover_refused(nodes, group, &mailbox, id, seal_key)
                .await?;
        }
        Ok(())
    }

    async fn recover_refused(
        &mut self,
        nodes: &mut [Node],
        group: &[u8],
        mailbox: &[u8; 32],
        id: [u8; 16],
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let Some((old, key, hlc, text)) = self.store.text_for_recovery(mailbox, group, &id)? else {
            return Ok(());
        };
        let body = chat::Body::Text {
            id: key.id,
            hlc,
            text,
        };
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let group_state = self
            .groups
            .iter_mut()
            .find(|known| known.id() == group)
            .ok_or(Error::UnknownGroup)?;
        let deliveries = chat::message(group_state, device, &body)?;
        let writes = deliveries
            .into_iter()
            .filter(|delivery| delivery.mailbox.as_slice() == mailbox)
            .map(|delivery| crate::prepare_put(&delivery.mailbox, delivery.mls))
            .collect::<Result<Vec<_>, _>>()?;
        let [write] = writes.as_slice() else {
            return Err(crate::Error::Input("recovery needs one recipient write").into());
        };
        let sealed = device.seal(seal_key)?;
        let device_key = device.device_key()?;
        self.store.replace_recovered_text(
            &old,
            std::slice::from_ref(write),
            &sealed,
            group,
            &body,
            &device_key,
        )?;
        let now_ms = i64::try_from(crate::now_ms()).map_err(crate::clock_range)?;
        self.deliver(nodes, write.clone(), now_ms, seal_key).await
    }
}
