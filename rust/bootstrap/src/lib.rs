//! The dyappd node: configuration, stores, request handling and the libp2p event loop.

pub mod catch_up;
pub mod claims;
pub mod config;
pub mod deny;
/// Error and result types.
pub mod error;
pub mod maintenance;
pub mod media;
mod media_repair;
pub mod metrics;
pub mod node;
pub mod places;
pub mod pow;
pub mod puzzle;
/// Per-peer, per-group and byte rate limits.
pub mod rate_limit;
/// Erasure coding of media blobs.
pub mod replication;
pub mod search;
pub mod secret;
pub mod service;
/// Profile and mailbox stores.
pub mod storage;

pub use config::NodeConfig;
pub use error::{BootstrapError, Result};
pub use storage::BootstrapStore;

/// Bytes from the OS random source; a node cannot run without one.
pub(crate) fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).expect("OS random source");
    bytes
}
