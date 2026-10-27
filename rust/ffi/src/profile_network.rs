//! A profile publisher uses only the selected identity folder's active network.

use crate::profile_draft::database;
use crate::profile_publish::{block_at, today, POW_BITS};
use crate::CoreError;
use crate::{IdentityKey, PublicProfileDraft};
use dyapp_client::publish;
use dyapp_client::store::Store;
use dyapp_client::Node;
use dyapp_identity::network::{self, Network, Settings};
use dyapp_p2p_net::{seed, Protocols};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

/// Public fields and version returned only after the shared core verifies signed readback.
#[derive(Clone, Debug, uniffi::Record)]
pub struct ProfilePublished {
    /// The verified public profile fields.
    pub draft: PublicProfileDraft,
    /// The verified published version.
    pub version: u64,
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

    /// Publish a solved draft through the active network and verify its signed readback.
    /// Any error leaves the saved draft available for retry.
    pub fn publish(
        &self,
        key: Arc<IdentityKey>,
        attested: bool,
    ) -> Result<ProfilePublished, CoreError> {
        let _serial = self.serial.lock().map_err(failed)?;
        let store = Store::open(&self.dir.join("client.db"), &key.identity).map_err(failed)?;
        let draft = store.profile_draft().map_err(failed)?.unwrap_or_default();
        if let Some(reason) = block_at(&draft, attested, today()?) {
            return Err(CoreError::Failed(format!("blocked: {reason}")));
        }
        if draft.pow_bits < POW_BITS {
            return Err(CoreError::Failed("unsolved".into()));
        }
        let network = active_network(&self.dir)?;
        let published = self.runtime.block_on(async {
            let mut node = connect(&network).await?;
            publish::publish(&store, &mut node, &key.identity, &draft)
                .await
                .map_err(failed)
        })?;
        Ok(ProfilePublished {
            draft: PublicProfileDraft::from(&published),
            version: published.version,
        })
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

    #[test]
    fn blocked_or_unconnected_publish_keeps_the_saved_draft() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-ffi-publish-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let key = IdentityKey::generate();
        crate::save_profile_draft(
            dir.to_str().unwrap().to_owned(),
            key.clone(),
            PublicProfileDraft {
                gender: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let publisher = ProfilePublisher::new(dir.to_str().unwrap().to_owned()).unwrap();
        let store = Store::open(&dir.join("client.db"), &key.identity).unwrap();
        let before = store.profile_draft().unwrap().unwrap();
        assert_eq!(
            publisher
                .publish(key.clone(), false)
                .unwrap_err()
                .to_string(),
            "blocked: notAttested"
        );
        assert_eq!(
            publisher
                .publish(key.clone(), true)
                .unwrap_err()
                .to_string(),
            "unsolved"
        );
        let mut solved = before.clone();
        solved.pow_bits = POW_BITS;
        store.save_profile_draft(&solved).unwrap();
        assert_eq!(
            publisher.publish(key, true).unwrap_err().to_string(),
            "notConnected"
        );
        assert_eq!(store.profile_draft().unwrap(), Some(solved));
    }
}
