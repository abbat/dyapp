#![no_main]
//! A call peer's SDP answer and ICE candidates, which `VideoSession` hands to the WebRTC parsers.

use rtc::ice::candidate::unmarshal_candidate;
use rtc::peer_connection::sdp::RTCSessionDescription;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = unmarshal_candidate(&text);
    let _ = RTCSessionDescription::answer(text.into_owned());
});
