#![no_main]
//! `/dyapp/profile` requests and the profile parser behind a valid signature.

use dyapp_fuzz::{sign, split, PEER, SERVICE};
use dyapp_p2p_net::proto::{profile_request::Request, ProfileRequest};
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = split(data) else {
        return;
    };
    let Ok(mut request) = ProfileRequest::decode(data) else {
        return;
    };
    if let Some(
        Request::Publish(record) | Request::Heartbeat(record) | Request::ReplicaPut(record),
    ) = &mut request.request
    {
        sign(selector, record);
        let _ = dyapp_profile::verify(record);
    }
    let _ = SERVICE.profile(&PEER, request);
});
