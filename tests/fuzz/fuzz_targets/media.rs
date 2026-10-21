#![no_main]
//! `/dyapp/media` requests: uploads, keeps, attachments, replicas and shards.

use dyapp_fuzz::{sign, split, PEER, SERVICE};
use dyapp_p2p_net::proto::{
    media_replica_put::Authorization, media_request::Request, MediaRequest,
};
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = split(data) else {
        return;
    };
    let Ok(mut request) = MediaRequest::decode(data) else {
        return;
    };
    let replica = match &mut request.request {
        Some(Request::Keep(record) | Request::Attach(record)) => {
            sign(selector, record);
            None
        }
        Some(Request::ReplicaPut(replica)) => Some(replica),
        Some(Request::ShardPut(put)) => put.replica.as_mut(),
        _ => None,
    };
    if let Some(Authorization::Keep(record) | Authorization::Attach(record)) =
        replica.and_then(|replica| replica.authorization.as_mut())
    {
        sign(selector, record);
    }
    let _ = SERVICE.media(&PEER, request);
});
