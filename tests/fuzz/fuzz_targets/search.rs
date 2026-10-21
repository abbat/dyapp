#![no_main]
//! `/dyapp/search` requests: publishes, heartbeats, queries and gets.

use dyapp_fuzz::{sign, split, PEER, SERVICE};
use dyapp_p2p_net::proto::{search_request::Request, SearchRequest};
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = split(data) else {
        return;
    };
    let Ok(mut request) = SearchRequest::decode(data) else {
        return;
    };
    if let Some(Request::Publish(record) | Request::Heartbeat(record)) = &mut request.request {
        sign(selector, record);
    }
    let _ = SERVICE.search(&PEER, request);
});
