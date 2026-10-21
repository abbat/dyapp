//! Shared state of the fuzz targets: one bootstrap node with every role and no limits.

use dyapp_bootstrap::media::MediaStore;
use dyapp_bootstrap::search::SearchIndex;
use dyapp_bootstrap::service::{Peer, Service};
use dyapp_bootstrap::{BootstrapStore, NodeConfig};
use dyapp_identity::{Domain, Identity, SignedRecord};
use std::sync::{Arc, LazyLock};

const DOMAINS: [Domain; 8] = [
    Domain::Profile,
    Domain::Envelope,
    Domain::MailboxFetch,
    Domain::MailboxAck,
    Domain::MediaKeep,
    Domain::MediaAttach,
    Domain::Heartbeat,
    Domain::DenyList,
];

/// The signer of re-signed records; its peer ID is also the requesting peer.
pub static IDENTITY: LazyLock<Identity> = LazyLock::new(|| Identity::from_secret(&[7; 32]));

/// The requesting peer.
pub static PEER: LazyLock<Peer> = LazyLock::new(|| Peer {
    id: IDENTITY.peer_id(),
    group: "fuzz".into(),
    coarse: "fuzz".into(),
});

/// A node in a fresh directory with the media and search roles; the limits never refuse, so
/// inputs reach the parsers instead of the rate limiter.
pub static SERVICE: LazyLock<Service> = LazyLock::new(|| {
    let dir = std::env::temp_dir().join(format!("dyapp-fuzz-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = NodeConfig::default();
    config.storage.dir = dir.clone();
    config.network.id_pow_bits = 0;
    let limits = &mut config.limits;
    limits.min_free_mb = 0;
    limits.requests_per_second = u32::MAX;
    limits.media_requests_per_second = u32::MAX;
    limits.ip_group_requests_per_second = u32::MAX;
    limits.sender_puts_per_second = u32::MAX;
    limits.bytes_per_second = u64::MAX;
    limits.strikes_to_ban = u32::MAX;
    let store = BootstrapStore::new(dir.to_str().unwrap()).unwrap();
    let index = SearchIndex::open(&config.storage.search_path(), &config.search).unwrap();
    let mut service = Service::new(store, config);
    service.media = Some(MediaStore::open(&dir).unwrap());
    service.search = Some(Arc::new(index));
    service
});

/// Splits off the first byte. When its top bit is set, `record` is re-signed by [`IDENTITY`]
/// with the domain its low bits select, so the payload gets past the signature check.
pub fn split(data: &[u8]) -> Option<(u8, &[u8])> {
    data.split_first().map(|(first, rest)| (*first, rest))
}

/// Re-signs `record` as [`split`] describes.
pub fn sign(selector: u8, record: &mut SignedRecord) {
    if selector & 0x80 != 0 {
        let domain = DOMAINS[usize::from(selector) % DOMAINS.len()];
        *record = IDENTITY.sign(domain, std::mem::take(&mut record.payload));
    }
}
