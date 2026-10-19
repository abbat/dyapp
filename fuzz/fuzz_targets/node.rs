#![no_main]
//! `/dyapp/node` requests: info, deny list and TURN credentials.

use dyapp_fuzz::{PEER, SERVICE};
use dyapp_p2p_net::proto::NodeRequest;
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(request) = NodeRequest::decode(data) {
        SERVICE.node(&PEER, request);
    }
});
