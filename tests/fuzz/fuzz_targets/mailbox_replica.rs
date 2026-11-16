#![no_main]
//! Node-to-node mailbox repair: replicated envelopes and inventory ids from a node peer.
//!
//! Replica puts and inventories are DENIED to peers without a libp2p Peer ID that carries the
//! node's PoW, so the ordinary `mailbox` target cannot reach these parsers.

use dyapp_fuzz::{sign, split, NODE_PEER, SERVICE};
use dyapp_p2p_net::proto::{mailbox_request::Request, MailboxRequest};
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = split(data) else {
        return;
    };
    let Ok(mut request) = MailboxRequest::decode(data) else {
        return;
    };
    match request.request.as_mut() {
        Some(Request::ReplicaPut(batch)) => batch
            .envelopes
            .iter_mut()
            .for_each(|record| sign(selector, record)),
        Some(Request::Inventory(_)) => {}
        _ => return,
    }
    let _ = SERVICE.mailbox(&NODE_PEER, &mut None, request);
});
