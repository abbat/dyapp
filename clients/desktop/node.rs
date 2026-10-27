//! The store and search node the app runs on the open identity's active network while it is
//! open, when the user turns it on (ADR 0030). Its settings are `node.json` in the slot folder,
//! its key and stores are in `node/` next to them.

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

/// The running node: its slot folder, peer ID and serving task.
#[derive(Default)]
pub struct NodeState(Mutex<Option<(PathBuf, String, JoinHandle<()>)>>);

/// The settings of the slot folder `dir`, the defaults when never saved.
pub fn load(dir: &Path) -> Result<Settings, String> {
    match fs::read(dir.join("node.json")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(error.to_string()),
        Ok(text) => serde_json::from_slice(&text).map_err(|e| e.to_string()),
    }
}

pub fn save(dir: &Path, settings: &Settings) -> Result<(), String> {
    // Below 4 MB a store would get no whole megabyte.
    if settings.disk_mb < 4 || settings.max_connections < 1 {
        return Err("invalidLimits".into());
    }
    let text = serde_json::to_vec(settings).map_err(|e| e.to_string())?;
    let temp = dir.join("node.json.tmp");
    fs::write(&temp, text)
        .and_then(|()| fs::rename(&temp, dir.join("node.json")))
        .map_err(|e| e.to_string())
}

/// The node config for slot folder `dir` on `network`; everything not set here is the `dyappd`
/// default.
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
    config.network.name = network.name.clone();
    config.network.id_pow_bits = dyapp_p2p_net::id_pow_bits();
    config.limits.max_connections = settings.max_connections;
    config.limits.profiles_max_mb = settings.disk_mb / 4;
    config.limits.messages_max_mb = settings.disk_mb / 2;
    config.search.max_bytes = (settings.disk_mb / 4) << 20;
    config
}

/// Stops the running node and waits until its stores are closed.
pub async fn stop(state: &NodeState) {
    if let Some((_, _, task)) = state.0.lock().await.take() {
        task.abort();
        let _ = task.await;
    }
}

/// Restarts the node for slot folder `dir` as its settings say. `status` is `off`, `noNetwork`
/// (no active network yet) or `running` with the node's `peerId`.
pub async fn apply(state: &NodeState, dir: &Path) -> Result<HashMap<&'static str, String>, String> {
    stop(state).await;
    let settings = load(dir)?;
    if !settings.enabled {
        return Ok(HashMap::from([("status", "off".into())]));
    }
    let Some(network) = network::load(dir, NetworkFile::Active).map_err(|e| e.to_string())? else {
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
    *state.0.lock().await = Some((dir.to_owned(), peer_id.clone(), task));
    Ok(HashMap::from([
        ("status", "running".into()),
        ("peerId", peer_id),
    ]))
}

/// `running` with the `peerId` when the node of slot folder `dir` runs, else `off`.
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
        network::save(&dir, NetworkFile::Active, &public).unwrap();
        let started = apply(&state, &dir).await.unwrap();
        assert_eq!(started["status"], "running");
        assert_eq!(status(&state, &dir).await, started);
        assert_eq!(status(&state, &temp_dir("other")).await["status"], "off");
        // A restart reuses the key and takes the storage lock again.
        assert_eq!(apply(&state, &dir).await.unwrap(), started);

        stop(&state).await;
        assert_eq!(status(&state, &dir).await["status"], "off");
        config(&dir, &on, &public).storage.lock().unwrap();
    }
}
