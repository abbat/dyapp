//! Preserve the frozen signed authorization when building whole or shard writes.
use super::Repair;
use dyapp_p2p_net::proto::{self, media_request::Request};

impl Repair {
    pub(super) fn write(&self, blob: usize, index: Option<u32>) -> Option<Request> {
        let blob = &self.blobs[blob];
        let authorization = Some(proto::media_replica_put::Authorization::Keep(
            (*self.record).clone(),
        ));
        let replica = proto::MediaReplicaPut {
            data: if blob.whole {
                blob.data.as_ref()?.clone()
            } else {
                Vec::new()
            },
            manifest: if blob.whole {
                None
            } else {
                blob.manifest.clone()
            },
            authorization,
        };
        Some(match index {
            None => Request::ReplicaPut(replica),
            Some(index) => Request::ShardPut(proto::MediaShardPut {
                replica: Some(replica),
                index,
                data: blob.encoded.get(index as usize)?.clone(),
            }),
        })
    }
}
