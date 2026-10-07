pub mod api;
pub mod error;
pub mod rate_limit;
pub mod replication;
pub mod storage;

pub use api::BootstrapServer;
pub use error::{BootstrapError, Result};
pub use storage::{BootstrapStore, MessageBlob};

#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    pub listen_addr: String,
    pub listen_port: u16,
    pub storage_path: String,
    pub max_peers: usize,
    pub replication_factor: usize,
    pub message_ttl_hours: u32,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0".to_string(),
            listen_port: 7070,
            storage_path: "/tmp/bootstrap-data".to_string(),
            max_peers: 1000,
            replication_factor: 3,
            message_ttl_hours: 24,
        }
    }
}
