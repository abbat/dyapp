use libp2p::identity::Keypair;
use libp2p::PeerId;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Leading zero bits of SHA-256 over a node's peer ID bytes: the static S/Kademlia puzzle that
/// makes every node ID cost about a minute on 2 vCPU (ADR 0008).
pub const ID_POW_BITS: u32 = 22;

// The unit tests route random peer IDs.
static POW_BITS: AtomicU32 = AtomicU32::new(if cfg!(test) { 0 } else { ID_POW_BITS });

/// Overrides [`ID_POW_BITS`] for this process: a test network sets it low.
pub fn set_id_pow_bits(bits: u32) {
    POW_BITS.store(bits, Ordering::Relaxed);
}

/// Required leading zero bits of a peer ID proof of work in this process.
pub fn id_pow_bits() -> u32 {
    POW_BITS.load(Ordering::Relaxed)
}

/// Whether `peer` carries the node-ID proof of work of `bits` leading zero bits.
#[must_use]
pub fn id_has_pow(peer: &PeerId, bits: u32) -> bool {
    let hash = dyapp_identity::sha256(&peer.to_bytes());
    let zeros = hash
        .iter()
        .zip(0u32..)
        .find(|(byte, _)| **byte != 0)
        .map_or(256, |(byte, index)| {
            index.saturating_mul(8).saturating_add(byte.leading_zeros())
        });
    zeros >= bits
}

fn pow_worker(found: &AtomicBool, bits: u32) -> Option<Keypair> {
    while !found.load(Ordering::Relaxed) {
        let keypair = Keypair::generate_ed25519();
        if id_has_pow(&keypair.public().to_peer_id(), bits) {
            found.store(true, Ordering::Relaxed);
            return Some(keypair);
        }
    }
    None
}

/// A fresh Ed25519 key whose peer ID has the proof of work of `bits`, searched on every CPU.
///
/// # Panics
/// Panics if a search worker cannot be spawned or panics, or if every worker
/// returns without a key. Key generation also requires an available OS random source.
pub fn generate_pow_keypair(bits: u32) -> Keypair {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let found = AtomicBool::new(false);
    std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| s.spawn(|| pow_worker(&found, bits)))
            .collect();
        workers
            .into_iter()
            .find_map(|w| w.join().expect("key search thread panicked"))
            .expect("a worker found the key")
    })
}
