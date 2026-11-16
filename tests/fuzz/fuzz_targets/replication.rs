#![no_main]
//! Node-to-node media replication: replica, shard and inventory input from a trusted peer.
//!
//! `Service::media` denies these variants to every peer outside the node set, so the `media`
//! target cannot reach the parsers; this target calls the handlers as the network layer does
//! after that trust check.

use dyapp_fuzz::{sign, split, SERVICE};
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
        Some(Request::Have(have)) => {
            let _ = SERVICE.media_inventory(&have.items);
        }
        _ => {}
    }
});
