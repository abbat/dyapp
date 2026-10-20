#![no_main]
//! `/dyapp/digest` requests between search nodes.

use dyapp_fuzz::{PEER, SERVICE};
use dyapp_p2p_net::proto::DigestRequest;
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    SERVICE.search_peer(&PEER.id, true);
    if let Ok(request) = DigestRequest::decode(data) {
        let _ = SERVICE.search_digest(&PEER, request);
    }
});
