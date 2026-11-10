//! The installation's optional store/search node (ADR 0030). `node.json` and `node/` live at
//! the app config root. Its last explicitly applied active-network choice is saved in
//! `node-config/network`; opening, switching or removing an identity never changes the node.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use dyapp_bootstrap::config::Role;
use dyapp_bootstrap::{node, NodeConfig};
use dyapp_identity::network::{self, Settings as NetworkFile};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// The node settings as the page sends and receives them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Settings {
    pub enabled: bool,
    /// Disk the stores may fill, split between profiles, mailboxes and the search index.
    pub disk_mb: u64,
    pub max_connections: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            disk_mb: 1024,
            max_connections: 64,
        }
    }
}

type Running = (PathBuf, String, JoinHandle<()>);

/// The running node: its installation root, peer ID and serving task.
#[derive(Default)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct NodeState(Mutex<Option<Running>>);

/// The installation settings in `dir`, or defaults when never saved.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn load(dir: &Path) -> Result<Settings, String> {
    match fs::read(dir.join("node.json")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(error.to_string()),
        Ok(text) => serde_json::from_slice(&text).map_err(|e| e.to_string()),
    }
}

#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn save(dir: &Path, settings: &Settings) -> Result<(), String> {
    // Below 4 MB a store would get no whole megabyte.
    if settings.disk_mb < 4 || settings.max_connections < 1 {
        return Err("invalidLimits".into());
    }
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let text = serde_json::to_vec(settings).map_err(|e| e.to_string())?;
    let temp = dir.join("node.json.tmp");
    fs::write(&temp, text)
        .and_then(|()| fs::rename(&temp, dir.join("node.json")))
        .map_err(|e| e.to_string())
}

fn save_network(root: &Path, active: &network::Network) -> Result<(), String> {
    // Bootstrap reserves node/network for its storage-binding marker, not seed settings.
    let dir = root.join("node-config");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    network::save(&dir, NetworkFile::Active, active).map_err(|e| e.to_string())
}

/// The node config for installation root `dir`; remaining values use `dyappd` defaults.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn config(dir: &Path, settings: &Settings, network: &network::Network) -> NodeConfig {
    let mut config = NodeConfig::default();
    config.storage.dir = dir.join("node");
    // ponytail: a new port on every start, so peers relearn it; a saved port when it matters.
    config.listen = vec!["[::]:0".into(), "0.0.0.0:0".into()];
    config.roles = vec![Role::Store, Role::Search];
    // A checked network may keep rows that failed; the node starts with the ones that parse.
    config.seeds = network
        .seeds
        .iter()
        .filter(|seed| dyapp_p2p_net::seed::parse(seed).is_ok())
        .cloned()
        .collect();
    config.network.name.clone_from(&network.name);
    config.network.id_pow_bits = dyapp_p2p_net::id_pow_bits();
    config.limits.max_connections = settings.max_connections;
    config.limits.profiles_max_mb = settings.disk_mb / 4;
    config.limits.messages_max_mb = settings.disk_mb / 2;
    config.search.max_bytes = (settings.disk_mb / 4) << 20;
    config
}

/// Stops the running node and waits until its stores are closed.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn stop(state: &NodeState) {
    let mut running = state.0.lock().await;
    stop_running(&mut running).await;
}

async fn stop_running(running: &mut Option<Running>) {
    if let Some((_, _, task)) = running.take() {
        task.abort();
        let _ = task.await;
    }
}

/// Restarts the installation node as its settings say. `status` is `off`, `noNetwork`
/// (no active network yet) or `running` with the node's `peerId`.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn apply(state: &NodeState, dir: &Path) -> Result<HashMap<&'static str, String>, String> {
    configure(state, dir, None, None).await
}

/// Apply explicit installation settings/network changes as one serialized transition.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn configure(
    state: &NodeState,
    dir: &Path,
    settings: Option<Settings>,
    active: Option<network::Network>,
) -> Result<HashMap<&'static str, String>, String> {
    let mut running = state.0.lock().await;
    if let Some(settings) = settings {
        save(dir, &settings)?;
    }
    if let Some(active) = active {
        save_network(dir, &active)?;
    }
    restart(&mut running, dir).await
}

async fn restart(
    running: &mut Option<Running>,
    dir: &Path,
) -> Result<HashMap<&'static str, String>, String> {
    stop_running(running).await;
    let settings = load(dir)?;
    if !settings.enabled {
        return Ok(HashMap::from([("status", "off".into())]));
    }
    let Some(network) =
        network::load(&dir.join("node-config"), NetworkFile::Active).map_err(|e| e.to_string())?
    else {
        return Ok(HashMap::from([("status", "noNetwork".into())]));
    };
    let config = config(dir, &settings, &network);
    let blocking = config.clone();
    // The key's proof of work takes a while once, so it runs off the async threads.
    tokio::task::spawn_blocking(move || {
        blocking.validate().map_err(|e| e.to_string())?;
        if !blocking.storage.key_path().exists() {
            blocking.keygen(None).map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    let opened = node::open(config).await.map_err(|e| e.to_string())?;
    let peer_id = opened.peer_id.to_string();
    let task = tokio::spawn(opened.run());
    *running = Some((dir.to_owned(), peer_id.clone(), task));
    Ok(HashMap::from([
        ("status", "running".into()),
        ("peerId", peer_id),
    ]))
}

/// `running` with its `peerId` for this installation root, otherwise `off`.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn status(state: &NodeState, dir: &Path) -> HashMap<&'static str, String> {
    match &*state.0.lock().await {
        Some((running, peer_id, task)) if running == dir && !task.is_finished() => {
            HashMap::from([("status", "running".into()), ("peerId", peer_id.clone())])
        }
        _ => HashMap::from([("status", "off".into())]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-desktop-node-{tag}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn settings_default_off_and_derive_the_node_config() {
        let dir = temp_dir("settings");
        assert_eq!(load(&dir), Ok(Settings::default()));
        assert!(!Settings::default().enabled);
        let settings = Settings {
            enabled: true,
            disk_mb: 2048,
            max_connections: 10,
        };
        save(&dir, &settings).unwrap();
        assert_eq!(load(&dir), Ok(settings.clone()));
        let tiny = Settings {
            disk_mb: 3,
            ..settings.clone()
        };
        assert_eq!(save(&dir, &tiny), Err("invalidLimits".into()));

        let network = network::Network {
            seeds: vec!["seed.example.org:33445".into(), "seed".into()],
            name: "city".into(),
        };
        let config = config(&dir, &settings, &network);
        assert_eq!(config.storage.dir, dir.join("node"));
        assert_eq!(config.roles, vec![Role::Store, Role::Search]);
        assert_eq!(config.seeds, vec!["seed.example.org:33445".to_string()]);
        assert_eq!(config.network.name, "city");
        assert_eq!(config.limits.max_connections, 10);
        assert_eq!(config.limits.profiles_max_mb, 512);
        assert_eq!(config.limits.messages_max_mb, 1024);
        assert_eq!(config.search.max_bytes, 512 << 20);
    }

    async fn check_profile_changes(
        state: &NodeState,
        dir: &Path,
        profile: &Path,
        started: &HashMap<&'static str, String>,
    ) {
        let task_id = state.0.lock().await.as_ref().unwrap().2.id();
        let changed = network::Network {
            seeds: vec![],
            name: "other".into(),
        };
        network::save(profile, NetworkFile::Active, &changed).unwrap();
        let persisted = network::load(&dir.join("node-config"), NetworkFile::Active)
            .unwrap()
            .unwrap();
        assert_eq!(persisted.name, "");
        assert_eq!(status(state, dir).await, *started);
        assert_eq!(state.0.lock().await.as_ref().unwrap().2.id(), task_id);
        fs::remove_dir_all(profile).unwrap();
        assert_eq!(status(state, dir).await, *started);
        assert_eq!(state.0.lock().await.as_ref().unwrap().2.id(), task_id);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_node_runs_only_when_on_and_stops_with_its_stores_closed() {
        dyapp_p2p_net::set_id_pow_bits(0);
        let (dir, state) = (temp_dir("run"), NodeState::default());
        assert_eq!(apply(&state, &dir).await.unwrap()["status"], "off");
        let on = Settings {
            enabled: true,
            ..Settings::default()
        };
        save(&dir, &on).unwrap();
        assert_eq!(apply(&state, &dir).await.unwrap()["status"], "noNetwork");

        let public = network::Network {
            seeds: vec![],
            name: String::new(),
        };
        let profile = dir.join("profiles/first");
        fs::create_dir_all(&profile).unwrap();
        network::save(&profile, NetworkFile::Active, &public).unwrap();
        assert_eq!(apply(&state, &dir).await.unwrap()["status"], "noNetwork");
        let started = configure(&state, &dir, None, Some(public.clone()))
            .await
            .unwrap();
        assert_eq!(started["status"], "running");
        // Storage binding uses a different format from the saved seed-list configuration.
        assert_eq!(fs::read(dir.join("node/network")).unwrap(), [] as [u8; 0]);
        assert_eq!(status(&state, &dir).await, started);
        let invalid = Settings {
            disk_mb: 0,
            ..on.clone()
        };
        assert_eq!(
            configure(&state, &dir, Some(invalid), None).await,
            Err("invalidLimits".into())
        );
        assert_eq!(status(&state, &dir).await, started);
        check_profile_changes(&state, &dir, &profile, &started).await;
        assert_eq!(status(&state, &temp_dir("other")).await["status"], "off");
        // A restart reuses the key and takes the storage lock again.
        assert_eq!(apply(&state, &dir).await.unwrap(), started);

        stop(&state).await;
        assert_eq!(status(&state, &dir).await["status"], "off");
        config(&dir, &on, &public).storage.lock().unwrap();
    }
}
