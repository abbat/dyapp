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

/// A fresh Ed25519 key from the OS random number generator.
///
/// # Errors
/// Returns random-source or key-decoding failures.
pub fn generate_keypair() -> anyhow::Result<Keypair> {
    let identity = dyapp_identity::Identity::try_generate()?;
    let secret = libp2p::identity::ed25519::SecretKey::try_from_bytes(identity.secret())?;
    Ok(Keypair::from(libp2p::identity::ed25519::Keypair::from(
        secret,
    )))
}

fn pow_worker(found: &AtomicBool, bits: u32) -> anyhow::Result<Option<Keypair>> {
    while !found.load(Ordering::Relaxed) {
        let keypair = generate_keypair()?;
        if id_has_pow(&keypair.public().to_peer_id(), bits) {
            found.store(true, Ordering::Relaxed);
            return Ok(Some(keypair));
        }
    }
    Ok(None)
}

/// A fresh Ed25519 key whose peer ID has the proof of work of `bits`, searched on every CPU.
///
/// # Errors
/// Returns random-source, key-decoding, worker creation or worker join failures, or an error
/// if every worker returns without a key.
pub fn generate_pow_keypair(bits: u32) -> anyhow::Result<Keypair> {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let found = AtomicBool::new(false);
    std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                std::thread::Builder::new()
                    .spawn_scoped(s, || pow_worker(&found, bits))
            })
            .collect::<std::io::Result<_>>()?;
        for worker in workers {
            let candidate = worker
                .join()
                .map_err(|_| anyhow::anyhow!("key search thread panicked"))??;
            if let Some(keypair) = candidate {
                return Ok(keypair);
            }
        }
        anyhow::bail!("key search workers returned without a key")
    })
}
