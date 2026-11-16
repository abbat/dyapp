#![no_main]
//! Node-to-node media replication: replica, shard, inventory and keep catch-up input.
//!
//! `Service::media` denies these variants to every peer outside the node set, so the `media`
//! target cannot reach the parsers; this target calls storage handlers directly and sends keep
//! catch-up through `Service::media` as a node peer.

use dyapp_fuzz::{sign, split, NODE_PEER, SERVICE};
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
    match request.request.take() {
        Some(Request::ReplicaPut(mut replica)) => {
            if let Some(Authorization::Keep(record) | Authorization::Attach(record)) =
                replica.authorization.as_mut()
            {
                sign(selector, record);
            }
            let _ = SERVICE.store_media(&replica);
        }
        Some(Request::ShardPut(mut put)) => {
            if let Some(replica) = put.replica.as_mut() {
                if let Some(Authorization::Keep(record) | Authorization::Attach(record)) =
                    replica.authorization.as_mut()
                {
                    sign(selector, record);
                }
            }
            let _ = SERVICE.store_shard(&put);
        }
        Some(Request::KeepCatchup(mut segment)) => {
            for record in segment.ancestors.iter_mut().chain(segment.target.as_mut()) {
                sign(selector, record);
            }
            let _ = SERVICE.media(
                &NODE_PEER,
                MediaRequest {
                    request: Some(Request::KeepCatchup(segment)),
                },
            );
        }
        Some(Request::Have(have)) => {
            let _ = SERVICE.media_inventory(&have.items);
        }
        _ => {}
    }
});
