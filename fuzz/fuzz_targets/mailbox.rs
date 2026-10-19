#![no_main]
//! `/dyapp/mailbox` requests: envelopes and owner requests behind a valid signature.
// ponytail: each input gets a fresh challenge it cannot know, so fetch and ack stop at the
// nonce check; feed the issued nonce back in if those paths need depth.

use dyapp_fuzz::{sign, split, PEER, SERVICE};
use dyapp_p2p_net::proto::{mailbox_request::Request, MailboxRequest};
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = split(data) else {
        return;
    };
    let Ok(mut request) = MailboxRequest::decode(data) else {
        return;
    };
    if let Some(
        Request::Put(record)
        | Request::Fetch(record)
        | Request::Ack(record)
        | Request::ReplicaAck(record),
    ) = &mut request.request
    {
        sign(selector, record);
    }
    let _ = SERVICE.mailbox(&PEER, &mut None, request);
});
