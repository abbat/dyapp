#![no_main]
//! Search gossip messages, which arrive as raw bytes; decodable ones are re-signed first.

use dyapp_fuzz::{sign, split, SERVICE};
use dyapp_p2p_net::proto::{search_request::Request, SearchRequest};
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = split(data) else {
        return;
    };
    let Ok(mut request) = SearchRequest::decode(data) else {
        SERVICE.search_gossip(data);
        return;
    };
    if let Some(Request::Publish(record) | Request::Heartbeat(record)) = &mut request.request {
        sign(selector, record);
    }
    SERVICE.search_gossip(&request.encode_to_vec());
});
