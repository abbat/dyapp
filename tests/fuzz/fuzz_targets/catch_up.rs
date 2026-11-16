#![no_main]
//! Catch-up answers from another search node: digest hashes and inventory pages.
//!
//! The node decodes these replies and turns them into the next digest, inventory range or
//! nothing; a peer controls every response field and every hash length.

use dyapp_bootstrap::catch_up::{on_digest, on_inventory, Range, PAGE};
use dyapp_bootstrap::search::SEARCH_TOPICS;
use dyapp_p2p_net::proto::{DigestResponse, InventoryResponse, SearchHit};
use prost::Message;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, rest)) = data.split_first() else {
        return;
    };
    // Our own digests for the answered bucket; the peer's answer is compared with them.
    let local = vec![[*selector; 32]; 256];
    let topic = u32::from(*selector) % SEARCH_TOPICS;
    let bucket = (*selector & 1 == 0).then(|| selector.wrapping_mul(7));
    let answer = DigestResponse::decode(rest).ok();
    let _ = on_digest(&local, topic, bucket, answer.as_ref());
    let _ = on_digest(&local, topic, bucket, None);
    if let Ok(inventory) = InventoryResponse::decode(rest) {
        let mut page: Vec<([u8; 32], u64)> = inventory.have.iter().filter_map(keyed).collect();
        let before = rest.get(..rest.len().min(32)).unwrap_or_default().to_vec();
        let _ = on_inventory(
            &Range {
                topic,
                after: Vec::new(),
                before: before.clone(),
            },
            page.clone(),
        );
        // A full page exercises the next-page branch; a shorter one ends the range.
        if page.len() < PAGE {
            page.resize(PAGE, ([0; 32], 0));
        }
        let _ = on_inventory(
            &Range {
                topic,
                after: before,
                before: Vec::new(),
            },
            page,
        );
    }
});

fn keyed(hit: &SearchHit) -> Option<([u8; 32], u64)> {
    let key: [u8; 32] = hit.key.as_slice().try_into().ok()?;
    Some((key, hit.version))
}
