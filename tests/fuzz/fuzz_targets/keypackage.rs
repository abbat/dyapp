#![no_main]
//! `/dyapp/keypackage` requests: signed packages, fetches and node replication.

use dyapp_fuzz::{split, IDENTITY, NODE_PEER, PEER, SERVICE};
use dyapp_identity::Domain;
use dyapp_p2p_net::proto::{key_package_request::Request, KeyPackageRequest};
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = split(data) else {
        return;
    };
    let Ok(mut request) = KeyPackageRequest::decode(data) else {
        return;
    };
    if let Some(Request::Publish(record) | Request::ReplicaPut(record)) = &mut request.request {
        if selector & 0x80 != 0 {
            *record = IDENTITY.sign(
                Domain::KeyPackage,
                std::mem::take(&mut record.payload),
            );
        }
    }
    let peer = if matches!(request.request, Some(Request::ReplicaPut(_))) {
        &NODE_PEER
    } else {
        &PEER
    };
    let _ = SERVICE.keypackage(peer, request);
});
