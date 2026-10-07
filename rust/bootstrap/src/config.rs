//! Node configuration: defaults, then a TOML file, then `DYAPP_NODE__<SECTION>__<KEY>` variables.
//! Every field has a default, so a config written for an older node keeps working. Unknown keys
//! are logged and ignored, so a node rolled back to an older version still starts.

use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const ENV_PREFIX: &str = "DYAPP_NODE__";

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    /// libp2p listen addresses, one per transport.
    pub listen: Vec<String>,
    /// Addresses announced to peers when the operator knows them; AutoNAT confirms others.
    pub external: Vec<String>,
    pub roles: Vec<Role>,
    pub storage: StorageConfig,
    pub limits: Limits,
    pub maintenance: Maintenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Store,
    Media,
    Search,
    Turn,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Holds the node key and every store without its own path.
    pub dir: PathBuf,
    pub profiles: Option<PathBuf>,
    pub messages: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub message_ttl_hours: u32,
    pub requests_per_second: u32,
}

/// Store maintenance: see `BootstrapStore::maintain`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Maintenance {
    pub interval_minutes: u32,
    /// Free pages released per store and run (4 KiB each): bounds the I/O of one run.
    pub vacuum_pages: u32,
}

impl Default for Maintenance {
    fn default() -> Self {
        Self {
            interval_minutes: 60,
            vacuum_pages: 2048,
        }
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            listen: vec![
                "/ip4/0.0.0.0/tcp/7070".into(),
                "/ip4/0.0.0.0/udp/7070/quic-v1".into(),
            ],
            external: vec![],
            roles: vec![Role::Store],
            storage: StorageConfig::default(),
            limits: Limits::default(),
            maintenance: Maintenance::default(),
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            dir: "/var/lib/dyapp-node".into(),
            profiles: None,
            messages: None,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            message_ttl_hours: 24,
            requests_per_second: 100,
        }
    }
}

impl StorageConfig {
    pub fn profiles_path(&self) -> PathBuf {
        self.profiles
            .clone()
            .unwrap_or_else(|| self.dir.join("profiles.db"))
    }

    pub fn messages_path(&self) -> PathBuf {
        self.messages
            .clone()
            .unwrap_or_else(|| self.dir.join("messages.db"))
    }

    pub fn key_path(&self) -> PathBuf {
        self.dir.join("node.key")
    }

    fn stores(&self) -> [PathBuf; 2] {
        [self.profiles_path(), self.messages_path()]
    }
}

impl NodeConfig {
    /// Reads the file (if given) and the environment. Returns the config and the ignored keys.
    pub fn load(
        file: Option<&Path>,
        env: impl IntoIterator<Item = (String, String)>,
    ) -> anyhow::Result<(Self, Vec<String>)> {
        let mut table = match file {
            Some(path) => fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("config {}: {e}", path.display()))?
                .parse::<toml::Table>()
                .map_err(|e| anyhow::anyhow!("config {}: {e}", path.display()))?,
            None => toml::Table::new(),
        };
        for (name, value) in env {
            let Some(key) = name.strip_prefix(ENV_PREFIX) else {
                continue;
            };
            let path: Vec<String> = key.split("__").map(str::to_lowercase).collect();
            // A value that is not valid TOML (a bare path, for example) is taken as a string.
            let value = format!("v = {value}")
                .parse::<toml::Table>()
                .ok()
                .and_then(|mut t| t.remove("v"))
                .unwrap_or(toml::Value::String(value));
            let (last, sections) = path.split_last().expect("split yields at least one item");
            let mut node = &mut table;
            for section in sections {
                node = node
                    .entry(section.clone())
                    .or_insert_with(|| toml::Value::Table(toml::Table::new()))
                    .as_table_mut()
                    .ok_or_else(|| anyhow::anyhow!("{name}: {section} is not a section"))?;
            }
            node.insert(last.clone(), value);
        }
        let mut ignored = Vec::new();
        let config = serde_ignored::deserialize(table, |path| ignored.push(path.to_string()))
            .map_err(|e| anyhow::anyhow!("config: {e}"))?;
        Ok((config, ignored))
    }

    /// Checks everything that could otherwise fail at runtime. Creates missing directories.
    pub fn validate(&self) -> anyhow::Result<()> {
        if fs::metadata("/proc/self")?.uid() == 0 {
            anyhow::bail!("refusing to run as root: start the node as an unprivileged user");
        }
        for address in self.listen.iter().chain(&self.external) {
            address
                .parse::<Multiaddr>()
                .map_err(|e| anyhow::anyhow!("bad address {address}: {e}"))?;
        }
        if self.listen.is_empty() {
            anyhow::bail!("listen: at least one address is required");
        }
        if let Some(role) = self.roles.iter().find(|r| **r != Role::Store) {
            anyhow::bail!("role {role:?} is not implemented yet");
        }
        if self.limits.message_ttl_hours < 1 {
            anyhow::bail!("limits.message_ttl_hours must be at least 1");
        }
        if self.limits.requests_per_second < 1 {
            anyhow::bail!("limits.requests_per_second must be at least 1");
        }
        if self.maintenance.interval_minutes < 1 || self.maintenance.vacuum_pages < 1 {
            anyhow::bail!("maintenance.interval_minutes and vacuum_pages must be at least 1");
        }
        let dirs = self.storage.stores().map(|store| {
            store
                .parent()
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
        });
        for dir in dirs.iter().chain([&self.storage.dir]) {
            check_writable(dir)?;
        }
        Ok(())
    }

    /// Loads the node key, or creates it on first start. A key that does not belong to the stored
    /// data is refused: a new key is a new node, so the operator must clear the data first.
    pub fn node_key(&self) -> anyhow::Result<Keypair> {
        let key_path = self.storage.key_path();
        let id_path = self.storage.dir.join("node.id");
        let keypair = match fs::read(&key_path) {
            Ok(bytes) => Keypair::from_protobuf_encoding(&bytes)
                .map_err(|e| anyhow::anyhow!("{}: {e}", key_path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut data = self.storage.stores().into_iter().chain([id_path.clone()]);
                if let Some(store) = data.find(|s| s.exists()) {
                    anyhow::bail!(
                        "{} is missing but {} exists: a new key is a new node, \
                         delete the stores to start from scratch",
                        key_path.display(),
                        store.display()
                    );
                }
                let keypair = Keypair::generate_ed25519();
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&key_path)?
                    .write_all(&keypair.to_protobuf_encoding()?)?;
                fs::write(&id_path, keypair.public().to_peer_id().to_string())?;
                keypair
            }
            Err(e) => return Err(anyhow::anyhow!("{}: {e}", key_path.display())),
        };
        let peer_id = keypair.public().to_peer_id();
        let stored: PeerId = fs::read_to_string(&id_path)
            .map_err(|e| anyhow::anyhow!("{}: {e}", id_path.display()))?
            .trim()
            .parse()
            .map_err(|e| anyhow::anyhow!("{}: {e}", id_path.display()))?;
        if stored != peer_id {
            anyhow::bail!(
                "{} does not match the data in {} (node {stored}): \
                 a new key is a new node, delete the data to start from scratch",
                key_path.display(),
                self.storage.dir.display()
            );
        }
        Ok(keypair)
    }
}

fn check_writable(dir: &Path) -> anyhow::Result<()> {
    let probe = dir.join(".write-probe");
    fs::create_dir_all(dir)
        .and_then(|()| fs::write(&probe, b""))
        .and_then(|()| fs::remove_file(&probe))
        .map_err(|e| anyhow::anyhow!("{} is not writable by this user: {e}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        PathBuf::from(format!("/tmp/ai/test-config-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn file_then_env_overrides_and_unknown_keys() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("node.toml");
        fs::write(
            &file,
            "listen = [\"/ip4/127.0.0.1/tcp/7071\"]\nfuture_key = 1\n\
             [limits]\nmessage_ttl_hours = 48\n",
        )
        .unwrap();
        let env = [
            ("DYAPP_NODE__LIMITS__MESSAGE_TTL_HOURS".into(), "72".into()),
            ("DYAPP_NODE__STORAGE__DIR".into(), "/srv/node".into()),
            ("OTHER".into(), "x".into()),
        ];
        let (config, ignored) = NodeConfig::load(Some(&file), env).unwrap();
        assert_eq!(config.listen, ["/ip4/127.0.0.1/tcp/7071"]);
        assert_eq!(config.limits.message_ttl_hours, 72);
        assert_eq!(config.limits.requests_per_second, 100);
        assert_eq!(config.storage.dir, PathBuf::from("/srv/node"));
        assert_eq!(
            config.storage.messages_path(),
            PathBuf::from("/srv/node/messages.db")
        );
        assert_eq!(ignored, ["future_key"]);
    }

    #[test]
    fn invalid_config_fails() {
        let invalid = |edit: fn(&mut NodeConfig)| {
            let mut config = NodeConfig::default();
            config.storage.dir = temp_dir();
            edit(&mut config);
            config.validate().is_err()
        };
        assert!(!invalid(|_| {}));
        assert!(invalid(|c| c.listen = vec!["not an address".into()]));
        assert!(invalid(|c| c.roles = vec![Role::Turn]));
        assert!(invalid(|c| c.limits.message_ttl_hours = 0));
        assert!(invalid(
            |c| c.storage.messages = Some("/proc/messages.db".into())
        ));
        assert!(NodeConfig::load(None, [("DYAPP_NODE__ROLES".into(), "[\"x\"]".into())]).is_err());
    }

    #[test]
    fn node_key_is_bound_to_the_data() {
        let mut config = NodeConfig::default();
        config.storage.dir = temp_dir();
        config.validate().unwrap();
        let key = config.node_key().unwrap();
        let mode = fs::metadata(config.storage.key_path()).unwrap().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(
            config.node_key().unwrap().public(),
            key.public(),
            "the key is reused"
        );

        // A replaced key does not match the data; a deleted one is not silently recreated.
        let other = Keypair::generate_ed25519().to_protobuf_encoding().unwrap();
        fs::write(config.storage.key_path(), other).unwrap();
        config.node_key().unwrap_err();
        fs::remove_file(config.storage.key_path()).unwrap();
        config.node_key().unwrap_err();

        // A missing key next to existing stores is refused.
        let mut config = NodeConfig::default();
        config.storage.dir = temp_dir();
        config.validate().unwrap();
        fs::write(config.storage.profiles_path(), b"").unwrap();
        config.node_key().unwrap_err();
    }
}
