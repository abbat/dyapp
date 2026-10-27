//! A profile publisher uses only the selected identity folder's active network.

use crate::profile_draft::database;
use crate::CoreError;
use dyapp_client::Node;
use dyapp_identity::network::{self, Network, Settings};
use dyapp_p2p_net::{seed, Protocols};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

pub(crate) fn active_network(dir: &Path) -> Result<Network, CoreError> {
    network::load(dir, Settings::Active)
        .map_err(failed)?
        .ok_or_else(|| CoreError::Failed("notConnected".into()))
}

/// Dial the first answering bootstrap node over each row's TCP and then QUIC address.
pub(crate) async fn connect(network: &Network) -> Result<Node, CoreError> {
    let protocols = Arc::new(Protocols::new(&network.name).map_err(failed)?);
    let mut error = CoreError::Failed("notConnected".into());
    for row in &network.seeds {
        for address in seed::parse(row.trim()).into_iter().flatten() {
            match Node::connect(address, Arc::clone(&protocols)).await {
                Ok(node) => return Ok(node),
                Err(failed) => error = CoreError::Failed(failed.to_string()),
            }
        }
    }
    Err(error)
}

/// Per-identity publisher network session; its calls block and run off the UI thread.
#[derive(uniffi::Object)]
pub struct ProfilePublisher {
    pub(crate) dir: PathBuf,
    pub(crate) runtime: tokio::runtime::Runtime,
    pub(crate) serial: Mutex<()>,
}

#[uniffi::export]
impl ProfilePublisher {
    /// Prepare a private Tokio runtime for this existing identity directory.
    #[uniffi::constructor]
    pub fn new(dir: String) -> Result<Arc<Self>, CoreError> {
        database(&dir)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(failed)?;
        Ok(Arc::new(Self {
            dir: dir.into(),
            runtime,
            serial: Mutex::new(()),
        }))
    }

    /// Verify that an active network answers; never uses the unactivated draft settings.
    pub fn check_connection(&self) -> Result<(), CoreError> {
        let _serial = self.serial.lock().map_err(failed)?;
        let network = active_network(&self.dir)?;
        self.runtime.block_on(connect(&network)).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn only_active_settings_can_supply_a_publisher_node() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-ffi-publisher-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let publisher = ProfilePublisher::new(dir.to_str().unwrap().to_owned()).unwrap();
        let network = Network {
            seeds: Vec::new(),
            name: String::new(),
        };
        network::save(&dir, Settings::Draft, &network).unwrap();
        assert_eq!(
            publisher.check_connection().unwrap_err().to_string(),
            "notConnected"
        );
        network::save(&dir, Settings::Active, &network).unwrap();
        assert_eq!(
            publisher.check_connection().unwrap_err().to_string(),
            "notConnected"
        );
        assert!(!dir.join("client.db").exists());
    }
}
