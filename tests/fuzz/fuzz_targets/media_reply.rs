#![no_main]
//! Media answers: shard metadata, ownership records and catch-up progress.

use dyapp_bootstrap::replication::MediaManifestExt;
use dyapp_fuzz::{split, IDENTITY};
use dyapp_identity::{Domain, SignedRecord};
use dyapp_p2p_net::proto::{self, MediaResponse};
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = split(data) else {
        return;
    };
    let Ok(mut response) = MediaResponse::decode(data) else {
        return;
    };
    if let Some(inventory) = &response.inventory {
        for (index, manifest) in inventory.manifests.iter().enumerate() {
            let _ = manifest.data_shards();
            let _ = manifest.shard_size();
            let _ = manifest.billed_bytes();
            if index == 0 {
                let _ = manifest.accepts(usize::from(selector & 0x7f), &response.data);
            }
        }
    }
    if let Some(record) = response
        .keep_state
        .as_mut()
        .and_then(|state| state.record.as_mut())
    {
        verify_keep(record, selector & 0x80 != 0);
    }
});

fn verify_keep(record: &mut SignedRecord, resign: bool) {
    let domain = proto::MediaKeepIntent::decode(record.payload.as_slice())
        .ok()
        .filter(|keep| keep.expected.is_some())
        .map_or(Domain::MediaKeep, |_| Domain::MediaConditionalKeep);
    if resign {
        *record = IDENTITY.sign(domain, std::mem::take(&mut record.payload));
    }
    let _ = record.verify(domain);
    let _ = proto::MediaKeepIntent::decode(record.payload.as_slice());
}
