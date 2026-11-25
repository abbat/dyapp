#![no_main]
//! A deny-list answer from another bootstrap node.

use dyapp_fuzz::{split, IDENTITY};
use dyapp_identity::Domain;
use dyapp_p2p_net::proto::NodeResponse;
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = split(data) else {
        return;
    };
    let Ok(mut response) = NodeResponse::decode(data) else {
        return;
    };
    let Some(record) = response.deny_list.as_mut() else {
        return;
    };
    if selector & 0x80 != 0 {
        *record = IDENTITY.sign(
            Domain::DenyList,
            std::mem::take(&mut record.payload),
        );
    }
    let Ok(peer) = IDENTITY.peer_id().parse() else {
        return;
    };
    let _ = dyapp_bootstrap::deny::verify(record, &peer);
});
