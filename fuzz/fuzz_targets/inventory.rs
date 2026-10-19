#![no_main]
//! `/dyapp/inventory` requests between search nodes.

use dyapp_fuzz::{PEER, SERVICE};
use dyapp_p2p_net::proto::InventoryRequest;
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    SERVICE.search_peer(&PEER.id, true);
    if let Ok(request) = InventoryRequest::decode(data) {
        let _ = SERVICE.search_inventory(&PEER, request);
    }
});
