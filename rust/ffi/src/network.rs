//! Per-identity bootstrap settings and the seed check behind the first-run connection screen.
//!
//! Calls block their thread; the app runs them off the UI thread. A check saves the draft
//! first, so a crash keeps the user's rows, and only a check that reached a node of the network
//! can be activated.

use dyapp_client::seeds::{self, Row};
use dyapp_identity::network::{self, Network, Settings};
use dyapp_p2p_net::seed::SeedError;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::sync::oneshot;

/// Seed rows and network name, as stored for one identity.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NetworkSettings {
    /// `host:port` or `host:port/<peer ID>` rows, in the user's order.
    pub seeds: Vec<String>,
    /// The network name, empty for the public network.
    pub name: String,
}

impl From<NetworkSettings> for Network {
    fn from(settings: NetworkSettings) -> Self {
        Self {
            seeds: settings.seeds,
            name: settings.name,
        }
    }
}

impl From<Network> for NetworkSettings {
    fn from(network: Network) -> Self {
        Self {
            seeds: network.seeds,
            name: network.name,
        }
    }
}

/// Which of an identity's two settings files.
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum NetworkFile {
    /// The rows being edited.
    Draft,
    /// The configuration in use, written only by [`ConnectionCheck::activate`].
    Active,
}

/// What one seed row turned out to be; the UI words it from the variant.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SeedRow {
    /// A node of this network answered.
    Connected {
        /// The node's peer ID.
        peer_id: String,
    },
    /// The row is not `host:port`.
    InvalidAddress,
    /// The part after `/` is not a peer ID.
    InvalidPeerId,
    /// The row names the same addresses as an earlier one.
    Duplicate,
    /// No dial succeeded in time.
    Unreachable {
        /// The last dial error, for logs; empty on timeout.
        reason: String,
    },
    /// A node answered with another key than the row pins.
    WrongPeer,
    /// A node answered from another network name.
    WrongNetwork,
}

impl From<Row> for SeedRow {
    fn from(row: Row) -> Self {
        match row {
            Row::Connected(peer) => Self::Connected {
                peer_id: peer.to_string(),
            },
            Row::Invalid(SeedError::Address) => Self::InvalidAddress,
            Row::Invalid(SeedError::PeerId) => Self::InvalidPeerId,
            Row::Duplicate => Self::Duplicate,
            Row::Unreachable(reason) => Self::Unreachable { reason },
            Row::WrongPin => Self::WrongPeer,
            Row::WrongNetwork => Self::WrongNetwork,
        }
    }
}

/// Why a check or activation gave no rows.
#[derive(Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum CheckError {
    /// The draft has no seed rows.
    #[error("no seed rows")]
    Empty,
    /// The network name breaks ADR 0017.
    #[error("{reason}")]
    InvalidNetwork {
        /// Which rule the name breaks, for logs.
        reason: String,
    },
    /// [`ConnectionCheck::cancel`] stopped the check.
    #[error("cancelled")]
    Cancelled,
    /// No check of this object reached a node of its network.
    #[error("no successful check")]
    NotChecked,
    /// Storage or the network stack failed.
    #[error("{reason}")]
    Failed {
        /// The cause, for logs.
        reason: String,
    },
}

fn failed(error: impl ToString) -> CheckError {
    CheckError::Failed {
        reason: error.to_string(),
    }
}

/// Reads one settings file of the identity folder `dir`; `None` when it was never saved.
#[uniffi::export]
pub fn load_network(dir: String, file: NetworkFile) -> Result<Option<NetworkSettings>, CheckError> {
    let loaded = network::load(&PathBuf::from(dir), settings(file)).map_err(failed)?;
    Ok(loaded.map(Into::into))
}

/// Saves the draft of the identity folder `dir` without connecting.
#[uniffi::export]
pub fn save_network_draft(dir: String, draft: NetworkSettings) -> Result<(), CheckError> {
    network::save(&PathBuf::from(dir), Settings::Draft, &draft.into()).map_err(failed)
}

fn settings(file: NetworkFile) -> Settings {
    match file {
        NetworkFile::Draft => Settings::Draft,
        NetworkFile::Active => Settings::Active,
    }
}

/// Checks and activates the drafts of one identity folder.
#[derive(uniffi::Object)]
pub struct ConnectionCheck {
    dir: PathBuf,
    runtime: tokio::runtime::Runtime,
    // Stops the running check; a second concurrent run takes it over.
    cancel: Mutex<Option<oneshot::Sender<()>>>,
    // The last draft that reached a node of its network.
    checked: Mutex<Option<Network>>,
}

#[uniffi::export]
impl ConnectionCheck {
    /// A checker for the identity folder `dir`.
    #[uniffi::constructor]
    pub fn new(dir: String) -> Result<Arc<Self>, CheckError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(failed)?;
        Ok(Arc::new(Self {
            dir: dir.into(),
            runtime,
            cancel: Mutex::new(None),
            checked: Mutex::new(None),
        }))
    }

    /// Saves `draft`, then dials every row for up to `wait_ms` and reports each in order.
    /// Retry is another call; the identity key is never touched.
    pub fn run(&self, draft: NetworkSettings, wait_ms: u32) -> Result<Vec<SeedRow>, CheckError> {
        let draft = Network::from(draft);
        network::save(&self.dir, Settings::Draft, &draft).map_err(failed)?;
        let (stop, stopped) = oneshot::channel();
        *self.cancel.lock().unwrap_or_else(PoisonError::into_inner) = Some(stop);
        let rows = self.runtime.block_on(async {
            tokio::select! {
                rows = seeds::check(&draft, Duration::from_millis(wait_ms.into())) => Some(rows),
                _ = stopped => None,
            }
        });
        let rows = match rows.ok_or(CheckError::Cancelled)? {
            Ok(rows) => rows,
            Err(seeds::CheckError::NoSeeds) => return Err(CheckError::Empty),
            Err(seeds::CheckError::Name(reason)) => {
                return Err(CheckError::InvalidNetwork { reason })
            }
            Err(error) => return Err(failed(error)),
        };
        if rows.iter().any(|row| matches!(row, Row::Connected(_))) {
            *self.checked.lock().unwrap_or_else(PoisonError::into_inner) = Some(draft);
        }
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Stops a running check, which then fails with `Cancelled`; the saved files stay as they
    /// were.
    pub fn cancel(&self) {
        if let Some(stop) = self
            .cancel
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = stop.send(());
        }
    }

    /// Makes the last draft that reached a node the identity's active configuration.
    pub fn activate(&self) -> Result<NetworkSettings, CheckError> {
        let checked = self
            .checked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let network = checked.ok_or(CheckError::NotChecked)?;
        network::save(&self.dir, Settings::Active, &network).map_err(failed)?;
        Ok(network.into())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use dyapp_p2p_net::{build_configured_swarm, Mode, SwarmOptions};
    use libp2p::futures::StreamExt;
    use libp2p::identity::Keypair;
    use libp2p::multiaddr::Protocol;
    use libp2p::swarm::SwarmEvent;
    use std::net::TcpListener;

    fn temp_dir(tag: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-ffi-network-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_str().unwrap().into()
    }

    /// A public-network node on a background thread; returns its TCP port and peer ID.
    pub(crate) fn node() -> (u16, String) {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let options = SwarmOptions {
                    mode: Mode::Auto,
                    ..SwarmOptions::default()
                };
                let mut swarm =
                    build_configured_swarm(Keypair::generate_ed25519(), options).unwrap();
                let peer = swarm.local_peer_id().to_string();
                swarm
                    .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
                    .unwrap();
                loop {
                    if let SwarmEvent::NewListenAddr { address, .. } =
                        swarm.select_next_some().await
                    {
                        if let Some(Protocol::Tcp(port)) = address.iter().nth(1) {
                            sender.send((port, peer.clone())).unwrap();
                        }
                    }
                }
            });
        });
        receiver.recv().unwrap()
    }

    #[test]
    fn a_checked_draft_is_saved_and_only_a_connected_one_activates() {
        let dir = temp_dir("check");
        let check = ConnectionCheck::new(dir.clone()).unwrap();
        assert_eq!(load_network(dir.clone(), NetworkFile::Draft), Ok(None));
        assert_eq!(check.activate(), Err(CheckError::NotChecked));

        let closed = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let unreachable = NetworkSettings {
            seeds: vec![format!("127.0.0.1:{closed}"), "seed".into()],
            name: String::new(),
        };
        let rows = check.run(unreachable.clone(), 2000).unwrap();
        assert!(matches!(rows[0], SeedRow::Unreachable { .. }), "{rows:?}");
        assert_eq!(rows[1], SeedRow::InvalidAddress);
        assert_eq!(
            load_network(dir.clone(), NetworkFile::Draft),
            Ok(Some(unreachable))
        );
        assert_eq!(check.activate(), Err(CheckError::NotChecked));

        let empty = NetworkSettings {
            seeds: vec![],
            name: String::new(),
        };
        assert_eq!(check.run(empty, 2000), Err(CheckError::Empty));
        let bad_name = NetworkSettings {
            seeds: vec!["seed:1".into()],
            name: " city".into(),
        };
        assert!(matches!(
            check.run(bad_name, 2000),
            Err(CheckError::InvalidNetwork { .. })
        ));

        let (port, peer) = node();
        let reachable = NetworkSettings {
            seeds: vec![format!("127.0.0.1:{port}/{peer}")],
            name: String::new(),
        };
        let rows = check.run(reachable.clone(), 5000).unwrap();
        assert_eq!(rows, vec![SeedRow::Connected { peer_id: peer }]);
        assert_eq!(load_network(dir.clone(), NetworkFile::Active), Ok(None));
        assert_eq!(check.activate(), Ok(reachable.clone()));
        assert_eq!(load_network(dir, NetworkFile::Active), Ok(Some(reachable)));
    }

    #[test]
    fn cancel_stops_a_running_check() {
        let check = ConnectionCheck::new(temp_dir("cancel")).unwrap();
        let canceller = check.clone();
        let stop = std::thread::spawn(move || {
            for _ in 0..20 {
                std::thread::sleep(Duration::from_millis(100));
                canceller.cancel();
            }
        });
        // Accepts TCP but never answers the handshake, so the check waits for the minute.
        let mute = TcpListener::bind("127.0.0.1:0").unwrap();
        let silent = NetworkSettings {
            seeds: vec![mute.local_addr().unwrap().to_string()],
            name: String::new(),
        };
        let started = std::time::Instant::now();
        assert_eq!(check.run(silent, 60_000), Err(CheckError::Cancelled));
        assert!(started.elapsed() < Duration::from_secs(30));
        stop.join().unwrap();
    }
}
