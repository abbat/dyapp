//! Per-identity bootstrap settings and the seed check behind the first-run connection screen;
//! mirrors the Android FFI `ConnectionCheck`. A check saves the draft first, so a crash keeps the
//! user's rows, and only a check that reached a node of the network can be activated.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use dyapp_client::seeds::{self, Row};
use dyapp_identity::network::{self, Network, Settings};
use dyapp_p2p_net::seed::SeedError;
use tokio::sync::oneshot;

#[derive(Default)]
pub struct NetworkState {
    // Stops the running check; a second concurrent check takes it over.
    cancel: Mutex<Option<oneshot::Sender<()>>>,
    // The identity folder and the last draft of it that reached a node of its network.
    checked: Mutex<Option<(PathBuf, Network)>>,
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
pub fn load(dir: &Path, file: &str) -> Result<Option<Draft>, String> {
    let which = match file {
        "draft" => Settings::Draft,
        "active" => Settings::Active,
        _ => return Err("Unknown settings file".into()),
    };
    let loaded = network::load(dir, which).map_err(|e| e.to_string())?;
    Ok(loaded.map(Into::into))
}

pub fn save_draft(dir: &Path, draft: Draft) -> Result<(), String> {
    network::save(dir, Settings::Draft, &draft.into()).map_err(|e| e.to_string())
}

/// Saves `draft`, then dials every row for up to `wait` and reports each in order. Errors are
/// `empty`, `cancelled`, `invalidNetwork: <reason>` or a storage or network message.
pub async fn check(
    state: &NetworkState,
    dir: PathBuf,
    draft: Draft,
    wait: Duration,
) -> Result<Vec<HashMap<&'static str, String>>, String> {
    let draft = Network::from(draft);
    network::save(&dir, Settings::Draft, &draft).map_err(|e| e.to_string())?;
    let (stop, stopped) = oneshot::channel();
    *state.cancel.lock().map_err(|e| e.to_string())? = Some(stop);
    let rows = tokio::select! {
        rows = seeds::check(&draft, wait) => rows,
        _ = stopped => return Err("cancelled".into()),
    };
    let rows = match rows {
        Ok(rows) => rows,
        Err(seeds::CheckError::NoSeeds) => return Err("empty".into()),
        Err(seeds::CheckError::Name(reason)) => return Err(format!("invalidNetwork: {reason}")),
        Err(error) => return Err(error.to_string()),
    };
    if rows.iter().any(|row| matches!(row, Row::Connected(_))) {
        *state.checked.lock().map_err(|e| e.to_string())? = Some((dir, draft));
    }
    Ok(rows.into_iter().map(describe).collect())
}

/// Stops a running check, which then fails with `cancelled`; the saved files stay as they were.
pub fn cancel(state: &NetworkState) {
    if let Some(stop) = state
        .cancel
        .lock()
        .ok()
        .and_then(|mut cancel| cancel.take())
    {
        let _ = stop.send(());
    }
}

/// Dials the active settings of `dir` as the app opens; `true` when a node of the network
/// answered, `false` when none did or nothing is active. Never touches the saved files.
pub async fn connect(dir: &Path, wait: Duration) -> Result<bool, String> {
    let Some(active) = network::load(dir, Settings::Active).map_err(|e| e.to_string())? else {
        return Ok(false);
    };
    let rows = seeds::check(&active, wait).await.unwrap_or_default();
    Ok(rows.iter().any(|row| matches!(row, Row::Connected(_))))
}

/// Makes the last draft of `dir` that reached a node the active configuration; `notChecked`
/// when no check of this folder did.
pub fn activate(state: &NetworkState, dir: &Path) -> Result<Draft, String> {
    let checked = state.checked.lock().map_err(|e| e.to_string())?.clone();
    let network = match checked {
        Some((checked_dir, network)) if checked_dir == dir => network,
        _ => return Err("notChecked".into()),
    };
    network::save(dir, Settings::Active, &network).map_err(|e| e.to_string())?;
    Ok(network.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-desktop-network-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn draft(seeds: &[&str], name: &str) -> Draft {
        Draft {
            seeds: seeds.iter().map(|seed| seed.to_string()).collect(),
            name: name.into(),
        }
    }

    #[tokio::test]
    async fn only_a_connected_check_activates_and_drafts_are_saved() {
        let (dir, state) = (temp_dir("check"), NetworkState::default());
        assert_eq!(load(&dir, "draft"), Ok(None));
        assert!(load(&dir, "other").is_err());
        assert_eq!(activate(&state, &dir), Err("notChecked".into()));

        let closed = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let unreachable = draft(&[&closed.to_string(), "seed"], "");
        let wait = Duration::from_secs(2);
        let rows = check(&state, dir.clone(), unreachable.clone(), wait)
            .await
            .unwrap();
        assert_eq!(rows[0]["status"], "unreachable");
        assert_eq!(rows[1]["status"], "invalidAddress");
        assert_eq!(load(&dir, "draft"), Ok(Some(unreachable)));
        assert_eq!(activate(&state, &dir), Err("notChecked".into()));

        let empty = check(&state, dir.clone(), draft(&[], ""), wait).await;
        assert_eq!(empty, Err("empty".into()));
        let bad_name = check(&state, dir.clone(), draft(&["seed:1"], " city"), wait).await;
        assert!(bad_name.unwrap_err().starts_with("invalidNetwork: "));

        // A remembered success never activates another identity's folder.
        let other = draft(&["seed:1"], "");
        *state.checked.lock().unwrap() = Some((dir.clone(), other.clone().into()));
        assert_eq!(
            activate(&state, &temp_dir("other")),
            Err("notChecked".into())
        );
        assert_eq!(activate(&state, &dir), Ok(other.clone()));
        assert_eq!(load(&dir, "active"), Ok(Some(other)));
    }

    #[tokio::test]
    async fn launch_connect_is_offline_without_an_answering_active_network() {
        let dir = temp_dir("launch");
        let wait = Duration::from_secs(2);
        assert_eq!(connect(&dir, wait).await, Ok(false));
        let closed = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let down = draft(&[&closed.to_string()], "");
        network::save(&dir, Settings::Active, &down.clone().into()).unwrap();
        assert_eq!(connect(&dir, wait).await, Ok(false));
        assert_eq!(load(&dir, "draft"), Ok(None));
    }

    #[tokio::test]
    async fn cancel_stops_a_running_check() {
        let (dir, state) = (temp_dir("cancel"), NetworkState::default());
        // Accepts TCP but never answers the handshake, so the check waits for the minute.
        let mute = TcpListener::bind("127.0.0.1:0").unwrap();
        let silent = draft(&[&mute.local_addr().unwrap().to_string()], "");
        let started = std::time::Instant::now();
        let running = check(&state, dir, silent, Duration::from_secs(60));
        let stop = async {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                cancel(&state);
            }
        };
        let result = tokio::select! {
            result = running => result,
            _ = stop => unreachable!(),
        };
        assert_eq!(result, Err("cancelled".into()));
        assert!(started.elapsed() < Duration::from_secs(30));
    }
}
