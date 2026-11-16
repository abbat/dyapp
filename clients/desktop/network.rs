//! Per-identity bootstrap settings and the seed check behind the first-run connection screen;
//! delegates to the same core checker as native clients. A check saves the draft first, so a crash keeps the
//! user's rows, and only a check that reached a node of the network can be activated.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use dyapp_client::cancellation::Cancellation;
use dyapp_client::network_check::{self, Checks, Error as CheckError};
use dyapp_client::seeds::Row;
use dyapp_identity::network::{self, Network, Settings};
use dyapp_p2p_net::seed::SeedError;

#[derive(Default)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct NetworkState {
    checks: Checks,
}

impl NetworkState {
    /// A successful check is valid only for the active identity generation.
    #[expect(unreachable_pub, reason = "Retained module API visibility")]
    pub fn clear_checked(&self) -> Result<(), String> {
        self.checks
            .clear_checked()
            .map_err(|error| error.to_string())
    }
}

/// Seed rows and network name as the page sends and receives them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Draft {
    pub seeds: Vec<String>,
    pub name: String,
}

impl From<Draft> for Network {
    fn from(draft: Draft) -> Self {
        Self {
            seeds: draft.seeds,
            name: draft.name,
        }
    }
}

impl From<Network> for Draft {
    fn from(network: Network) -> Self {
        Self {
            seeds: network.seeds,
            name: network.name,
        }
    }
}

/// The page's view of one row: `status` names the variant, the UI words it.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn describe(row: Row) -> HashMap<&'static str, String> {
    let status = match row {
        Row::Connected(peer) => {
            return HashMap::from([("status", "connected".into()), ("peerId", peer.to_string())])
        }
        Row::Unreachable(reason) => {
            return HashMap::from([("status", "unreachable".into()), ("reason", reason)])
        }
        Row::Invalid(SeedError::Address) => "invalidAddress",
        Row::Invalid(SeedError::PeerId) => "invalidPeerId",
        Row::Duplicate => "duplicate",
        Row::WrongPin => "wrongPeer",
        Row::WrongNetwork => "wrongNetwork",
    };
    HashMap::from([("status", status.into())])
}

/// Reads the `draft` or `active` settings of the identity folder `dir`; `None` if never saved.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn load(dir: &Path, file: &str) -> Result<Option<Draft>, String> {
    let which = match file {
        "draft" => Settings::Draft,
        "active" => Settings::Active,
        _ => return Err("Unknown settings file".into()),
    };
    let loaded = network::load(dir, which).map_err(|e| e.to_string())?;
    Ok(loaded.map(Into::into))
}

#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn save_draft(dir: &Path, draft: Draft) -> Result<(), String> {
    network::save(dir, Settings::Draft, &draft.into()).map_err(|e| e.to_string())
}

/// Saves `draft`, then dials every row for up to `wait` and reports each in order. Errors are
/// `empty`, `cancelled`, `invalidNetwork: <reason>` or a storage or network message.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn check(
    state: &NetworkState,
    dir: PathBuf,
    draft: Draft,
    wait: Duration,
    transition_started: impl FnOnce() -> Result<bool, String>,
) -> Result<Vec<HashMap<&'static str, String>>, String> {
    let cancellation = Cancellation::new();
    state
        .checks
        .check(dir, draft.into(), wait, &cancellation, || {
            transition_started().map_err(CheckError::Failed)
        })
        .await
        .map(|rows| rows.into_iter().map(describe).collect())
        .map_err(|error| error.to_string())
}

/// Stops a running check, which then fails with `cancelled`; the saved files stay as they were.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn cancel(state: &NetworkState) {
    state.checks.cancel();
}

/// Dials the active settings of `dir` as the app opens; `true` when a node of the network
/// answered, `false` when none did or nothing is active. Never touches the saved files.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn connect(dir: &Path, wait: Duration) -> Result<bool, String> {
    network_check::connect(dir, wait)
        .await
        .map_err(|error| error.to_string())
}

/// Makes the last draft of `dir` that reached a node the active configuration; `notChecked`
/// when no check of this folder did.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn activate(state: &NetworkState, dir: &Path) -> Result<Draft, String> {
    state
        .checks
        .activate(dir)
        .map(Into::into)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/desktop-network-tests")
            .join(tag)
            .join(dyapp_identity::Identity::generate().peer_id());
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn reserved_endpoint() -> (TcpListener, std::net::UdpSocket) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
            match std::net::UdpSocket::bind(tcp.local_addr().unwrap()) {
                Ok(udp) => return (tcp, udp),
                // Windows may reject an ephemeral UDP port reserved by the OS.
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied
                    ) && std::time::Instant::now() < deadline => {}
                Err(error) => panic!("reserve TCP/QUIC fixture: {error}"),
            }
        }
    }

    fn draft(seeds: &[&str], name: &str) -> Draft {
        Draft {
            seeds: seeds.iter().map(ToString::to_string).collect(),
            name: name.into(),
        }
    }

    #[tokio::test]
    async fn failed_checks_keep_drafts_without_allowing_activation() {
        let (dir, state) = (temp_dir("check"), NetworkState::default());
        assert_eq!(load(&dir, "draft"), Ok(None));
        assert!(load(&dir, "other").is_err());
        assert_eq!(activate(&state, &dir), Err("notChecked".into()));

        let (silent, _udp) = reserved_endpoint();
        let closed = silent.local_addr().unwrap();
        let unreachable = draft(&[&closed.to_string(), "seed"], "");
        let wait = Duration::from_secs(2);
        let rows = check(&state, dir.clone(), unreachable.clone(), wait, || Ok(false))
            .await
            .unwrap();
        assert_eq!(rows[0]["status"], "unreachable");
        assert_eq!(rows[1]["status"], "invalidAddress");
        assert_eq!(load(&dir, "draft"), Ok(Some(unreachable)));
        assert_eq!(activate(&state, &dir), Err("notChecked".into()));

        let empty = check(&state, dir.clone(), draft(&[], ""), wait, || Ok(false)).await;
        assert_eq!(empty, Err("empty".into()));
        let bad_name = check(
            &state,
            dir.clone(),
            draft(&["seed:1"], " city"),
            wait,
            || Ok(false),
        )
        .await;
        assert!(bad_name.unwrap_err().starts_with("invalidNetwork: "));
    }

    #[tokio::test]
    async fn launch_connect_is_offline_without_an_answering_active_network() {
        let dir = temp_dir("launch");
        let wait = Duration::from_secs(2);
        assert_eq!(connect(&dir, wait).await, Ok(false));
        let (silent, _udp) = reserved_endpoint();
        let closed = silent.local_addr().unwrap();
        let down = draft(&[&closed.to_string()], "");
        network::save(&dir, Settings::Active, &down.clone().into()).unwrap();
        assert_eq!(connect(&dir, wait).await, Ok(false));
        assert_eq!(load(&dir, "draft"), Ok(None));
    }

    #[tokio::test]
    async fn check_registered_after_transition_signal_cancels_before_dial() {
        use crate::identity_gate::IdentityGate;

        let (dir, state) = (temp_dir("transition"), NetworkState::default());
        let gate = IdentityGate::new(Some("first"));
        let lease = gate.lease().unwrap();
        let transition = gate.start_transition().unwrap();
        cancel(&state); // The signal arrives before the check registers its sender.
        let first = draft(&[], "");
        let result = check(&state, dir.clone(), first.clone(), Duration::ZERO, || {
            Ok(lease.transitioning().unwrap())
        })
        .await;
        assert_eq!(result, Err("cancelled".into()));
        assert_eq!(load(&dir, "draft"), Ok(Some(first)));

        drop(lease);
        transition.wait().unwrap();
        transition.commit(Some("second")).unwrap();
        let next = gate.lease().unwrap();
        let result = check(&state, dir, draft(&[], ""), Duration::ZERO, || {
            Ok(next.transitioning().unwrap())
        })
        .await;
        assert_eq!(result, Err("empty".into()));
    }

    #[tokio::test]
    async fn cancel_at_registration_wins_without_a_readiness_sleep() {
        let (dir, state) = (temp_dir("cancel"), NetworkState::default());
        let draft = draft(&["not an address"], "");
        let result = check(&state, dir.clone(), draft.clone(), Duration::ZERO, || {
            cancel(&state);
            Ok(false)
        })
        .await;
        assert_eq!(result, Err("cancelled".into()));
        assert_eq!(load(&dir, "draft"), Ok(Some(draft)));
        assert_eq!(activate(&state, &dir), Err("notChecked".into()));
    }
}
