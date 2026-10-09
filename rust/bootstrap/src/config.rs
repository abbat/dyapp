//! Node configuration: defaults, then a TOML file, then `DYAPPD__<SECTION>__<KEY>` variables.
//! Every field has a default, so a config written for an older node keeps working. Unknown keys
//! are logged and ignored, so a node rolled back to an older version still starts.

use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::SocketAddr;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const ENV_PREFIX: &str = "DYAPPD__";

/// The TCP and QUIC (UDP) addresses of a `host:port` listen or external address.
pub fn transports(address: &str) -> anyhow::Result<[Multiaddr; 2]> {
    let socket: SocketAddr = address.parse().map_err(|_| {
        anyhow::anyhow!("bad address {address}: expected host:port, e.g. [::]:7070 or 0.0.0.0:7070")
    })?;
    let ip = Multiaddr::from(socket.ip());
    Ok([
        ip.clone().with(Protocol::Tcp(socket.port())),
        ip.with(Protocol::Udp(socket.port())).with(Protocol::QuicV1),
    ])
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    /// `host:port` addresses, each served over TCP and QUIC; see [`transports`].
    pub listen: Vec<String>,
    /// `host:port` addresses announced to peers when the operator knows them; AutoNAT confirms
    /// others.
    pub external: Vec<String>,
    /// Nodes dialed at start to join the network; `/dnsaddr/<host>` reads `_dnsaddr.<host>` TXT.
    pub seeds: Vec<String>,
    pub roles: Vec<Role>,
    pub storage: StorageConfig,
    pub limits: Limits,
    pub maintenance: Maintenance,
    pub network: Network,
    pub turn: Turn,
}

/// The coturn relay next to the node (role turn), run with `use-auth-secret`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Turn {
    /// `turn:` / `turns:` URLs handed to peers.
    pub urls: Vec<String>,
    /// coturn's `static-auth-secret`; `DYAPPD__TURN__SECRET` keeps it out of the file.
    pub secret: String,
    /// Lifetime of the credentials the node hands out.
    pub credential_minutes: u32,
}

impl Default for Turn {
    fn default() -> Self {
        Self {
            urls: vec![],
            secret: String::new(),
            credential_minutes: 60,
        }
    }
}

impl Turn {
    fn validate(&self) -> anyhow::Result<()> {
        if self.secret.is_empty() || self.credential_minutes < 1 {
            anyhow::bail!("role turn needs turn.secret and turn.credential_minutes >= 1");
        }
        let turn_url = |u: &String| u.starts_with("turn:") || u.starts_with("turns:");
        if self.urls.is_empty() || !self.urls.iter().all(turn_url) {
            anyhow::bail!("role turn needs turn.urls, each starting with turn: or turns:");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Network {
    /// Node-ID proof of work required of the own key and of every routed peer. Lower it on test
    /// networks only: nodes of the public network do not route a key made with fewer bits.
    pub id_pow_bits: u32,
    /// At most one peer per /16 (IPv6 /32) in a k-bucket. Turn it off only on a network whose
    /// nodes share a /16, such as a test network.
    pub distinct_outbound_groups: bool,
    /// Minutes after the first connection before a peer gets replicas (repair, forwarded acks);
    /// 0 trusts at once.
    pub storage_trust_minutes: u32,
    /// Answer other nodes' deny-list requests with this node's signed list (no notes).
    pub share_deny_list: bool,
    /// Ask connected nodes for their deny lists each maintenance run and store them; the node
    /// takes no action on them.
    pub accept_deny_lists: bool,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            id_pow_bits: dyapp_p2p_net::ID_POW_BITS,
            distinct_outbound_groups: true,
            storage_trust_minutes: 60,
            share_deny_list: false,
            accept_deny_lists: false,
        }
    }
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
    /// The media directory: blob files and `media.db`.
    pub media: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub message_ttl_hours: u32,
    pub requests_per_second: u32,
    /// Live data per store; a full store answers `FULL` to writes.
    pub profiles_max_mb: u64,
    pub messages_max_mb: u64,
    /// Distinct media blob bytes, and blob bytes per owner.
    pub media_max_mb: u64,
    pub media_per_owner_mb: u64,
    /// How long a chat attachment the recipient never released is kept.
    pub attachment_retention_hours: u32,
    /// How long a profile or tombstone is kept after its owner's last signed action.
    pub profile_ttl_days: u32,
    /// Media requests per second from one peer: a reply carries up to 1 MiB.
    pub media_requests_per_second: u32,
    /// Free space kept on the file system of each store.
    pub min_free_mb: u64,
    /// Node protocol bytes in and out per calendar month (UTC); 0 = no cap.
    pub monthly_traffic_gb: u64,
    /// Node protocol bytes in and out per second, shed like the monthly cap; 0 = no limit.
    pub bytes_per_second: u64,
    pub max_connections: u32,
    pub max_connections_per_peer: u32,
    /// Concurrent streams per connection and protocol.
    pub max_streams: usize,
    /// New connections are refused while the process uses more physical memory; 0 = no limit.
    pub max_memory_mb: u64,
    /// Requests per second shared by all peers of one IP group: IPv4 and IPv6 addresses with
    /// the same leading `ipv4_prefix` / `ipv6_prefix` bits.
    pub ip_group_requests_per_second: u32,
    pub ipv4_prefix: u8,
    pub ipv6_prefix: u8,
    /// Envelopes per second accepted from one sender key.
    pub sender_puts_per_second: u32,
    /// Refused floods and bad signatures that ban a peer for `ban_minutes`.
    pub strikes_to_ban: u32,
    pub ban_minutes: u32,
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
            // libp2p binds IPv6 sockets v6-only, so dual stack takes both wildcards.
            listen: vec!["[::]:7070".into(), "0.0.0.0:7070".into()],
            external: vec![],
            seeds: vec![],
            roles: vec![Role::Store],
            storage: StorageConfig::default(),
            limits: Limits::default(),
            maintenance: Maintenance::default(),
            network: Network::default(),
            turn: Turn::default(),
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            dir: "/var/lib/dyappd".into(),
            profiles: None,
            messages: None,
            media: None,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            message_ttl_hours: 24,
            requests_per_second: 100,
            profiles_max_mb: 1024,
            messages_max_mb: 4096,
            media_max_mb: 10240,
            media_per_owner_mb: 10,
            attachment_retention_hours: 168,
            profile_ttl_days: 30,
            media_requests_per_second: 10,
            min_free_mb: 512,
            monthly_traffic_gb: 0,
            bytes_per_second: 0,
            max_connections: 1000,
            max_connections_per_peer: 4,
            max_streams: 16,
            max_memory_mb: 0,
            ip_group_requests_per_second: 1000,
            ipv4_prefix: 24,
            ipv6_prefix: 48,
            sender_puts_per_second: 10,
            strikes_to_ban: 100,
            ban_minutes: 10,
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

    pub fn media_path(&self) -> PathBuf {
        self.media.clone().unwrap_or_else(|| self.dir.join("media"))
    }

    pub fn key_path(&self) -> PathBuf {
        self.dir.join("node.key")
    }

    /// The routing-table peers saved by the last run, one multiaddr per line.
    pub fn peers_path(&self) -> PathBuf {
        self.dir.join("peers")
    }

    /// The last outbound peers that answered, dialled first on the next start.
    pub fn anchors_path(&self) -> PathBuf {
        self.dir.join("anchors")
    }

    /// The operator's deny list, edited by `dyappd deny`.
    pub fn deny_path(&self) -> PathBuf {
        self.dir.join("deny.db")
    }

    fn stores(&self) -> [PathBuf; 3] {
        [
            self.profiles_path(),
            self.messages_path(),
            self.media_path().join("media.db"),
        ]
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
            transports(address)?;
        }
        for address in &self.seeds {
            address
                .parse::<Multiaddr>()
                .map_err(|e| anyhow::anyhow!("bad address {address}: {e}"))?;
        }
        if self.listen.is_empty() {
            anyhow::bail!("listen: at least one address is required");
        }
        if let Some(role) = self
            .roles
            .iter()
            .find(|r| !matches!(r, Role::Store | Role::Media | Role::Turn))
        {
            anyhow::bail!("role {role:?} is not implemented yet");
        }
        // ponytail: a media node is found through the store role's key space; media-only nodes
        // need their own Kademlia protocol.
        if self.roles.contains(&Role::Media) && !self.roles.contains(&Role::Store) {
            anyhow::bail!("role media needs role store");
        }
        if self.roles.contains(&Role::Turn) {
            // Client-mode nodes serve no protocol, so only a store node hands out credentials.
            if !self.roles.contains(&Role::Store) {
                anyhow::bail!("role turn needs role store");
            }
            self.turn.validate()?;
        }
        let l = &self.limits;
        if l.message_ttl_hours < 1 || l.attachment_retention_hours < 1 || l.profile_ttl_days < 1 {
            anyhow::bail!(
                "limits.message_ttl_hours, attachment_retention_hours and profile_ttl_days must \
                 be at least 1"
            );
        }
        if self.limits.requests_per_second < 1 || self.limits.media_requests_per_second < 1 {
            anyhow::bail!(
                "limits.requests_per_second and media_requests_per_second must be at least 1"
            );
        }
        let l = &self.limits;
        if l.max_connections < 1 || l.max_connections_per_peer < 1 || l.max_streams < 1 {
            anyhow::bail!("limits.max_connections, max_connections_per_peer and max_streams must be at least 1");
        }
        if l.ip_group_requests_per_second < 1
            || l.sender_puts_per_second < 1
            || l.strikes_to_ban < 1
            || l.ban_minutes < 1
        {
            anyhow::bail!("limits.ip_group_requests_per_second, sender_puts_per_second, strikes_to_ban and ban_minutes must be at least 1");
        }
        if l.ipv4_prefix > 32 || l.ipv6_prefix > 128 {
            anyhow::bail!("limits.ipv4_prefix must be at most 32 and ipv6_prefix at most 128");
        }
        if self.network.id_pow_bits > 32 {
            anyhow::bail!("network.id_pow_bits must be at most 32");
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

    /// Makes the node key with the proof of work of `network.id_pow_bits` (about a minute on
    /// 2 vCPU) and binds the data directory to it. Refuses an existing key or existing data: a new
    /// key is a new node.
    pub fn keygen(&self) -> anyhow::Result<PeerId> {
        let key_path = self.storage.key_path();
        let id_path = self.storage.dir.join("node.id");
        let mut data = [key_path.clone(), id_path.clone()]
            .into_iter()
            .chain(self.storage.stores());
        if let Some(file) = data.find(|s| s.exists()) {
            anyhow::bail!(
                "{} exists: a new key is a new node, delete the data to start from scratch",
                file.display()
            );
        }
        let keypair = dyapp_p2p_net::generate_pow_keypair(self.network.id_pow_bits);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&key_path)
            .map_err(|e| anyhow::anyhow!("{}: {e}", key_path.display()))?
            .write_all(&keypair.to_protobuf_encoding()?)?;
        let peer_id = keypair.public().to_peer_id();
        fs::write(&id_path, peer_id.to_string())?;
        Ok(peer_id)
    }

    /// Loads the node key made by [`Self::keygen`]. A key that does not belong to the stored data
    /// or lacks the proof of work is refused.
    pub fn node_key(&self) -> anyhow::Result<Keypair> {
        let key_path = self.storage.key_path();
        let id_path = self.storage.dir.join("node.id");
        let keypair = match fs::read(&key_path) {
            Ok(bytes) => Keypair::from_protobuf_encoding(&bytes)
                .map_err(|e| anyhow::anyhow!("{}: {e}", key_path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => anyhow::bail!(
                "{} is missing: run `dyappd keygen` once and keep the key safe",
                key_path.display()
            ),
            Err(e) => return Err(anyhow::anyhow!("{}: {e}", key_path.display())),
        };
        let peer_id = keypair.public().to_peer_id();
        if !dyapp_p2p_net::id_has_pow(&peer_id, self.network.id_pow_bits) {
            anyhow::bail!(
                "{} lacks the node-ID proof of work of {} bits: make a new node with `dyappd keygen`",
                key_path.display(),
                self.network.id_pow_bits
            );
        }
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
            "listen = [\"127.0.0.1:7071\"]\nfuture_key = 1\n\
             [limits]\nmessage_ttl_hours = 48\n",
        )
        .unwrap();
        let env = [
            ("DYAPPD__LIMITS__MESSAGE_TTL_HOURS".into(), "72".into()),
            ("DYAPPD__STORAGE__DIR".into(), "/srv/node".into()),
            ("OTHER".into(), "x".into()),
        ];
        let (config, ignored) = NodeConfig::load(Some(&file), env).unwrap();
        assert_eq!(config.listen, ["127.0.0.1:7071"]);
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
        assert!(invalid(|c| c.listen = vec!["/ip4/0.0.0.0/tcp/7070".into()]));
        assert!(invalid(|c| c.external = vec!["[2001:db8::1]".into()]));
        assert_eq!(
            transports("[2001:db8::1]:7070")
                .unwrap()
                .map(|a| a.to_string()),
            [
                "/ip6/2001:db8::1/tcp/7070",
                "/ip6/2001:db8::1/udp/7070/quic-v1"
            ]
        );
        assert!(invalid(|c| c.seeds = vec!["seed.example".into()]));
        assert!(!invalid(|c| c.seeds = vec!["/dnsaddr/seed.example".into()]));
        fn turn(c: &mut NodeConfig) {
            c.roles = vec![Role::Store, Role::Turn];
            c.turn.secret = "s".into();
            c.turn.urls = vec!["turn:relay.example:3478".into()];
        }
        assert!(!invalid(turn));
        assert!(invalid(|c| {
            turn(c);
            c.roles = vec![Role::Turn];
        }));
        assert!(invalid(|c| {
            turn(c);
            c.turn.secret.clear();
        }));
        assert!(invalid(|c| {
            turn(c);
            c.turn.urls = vec!["relay.example:3478".into()];
        }));
        assert!(invalid(|c| c.roles = vec![Role::Media]));
        assert!(!invalid(|c| c.roles = vec![Role::Store, Role::Media]));
        assert!(invalid(|c| c.limits.message_ttl_hours = 0));
        assert!(invalid(|c| c.limits.attachment_retention_hours = 0));
        assert!(invalid(|c| c.limits.profile_ttl_days = 0));
        assert!(invalid(
            |c| c.storage.messages = Some("/proc/messages.db".into())
        ));
        assert!(NodeConfig::load(None, [("DYAPPD__ROLES".into(), "[\"x\"]".into())]).is_err());
    }

    #[test]
    fn node_key_is_bound_to_the_data() {
        let mut config = NodeConfig::default();
        config.storage.dir = temp_dir();
        config.network.id_pow_bits = 4;
        config.validate().unwrap();
        config.node_key().unwrap_err();
        let peer = config.keygen().unwrap();
        config.keygen().unwrap_err();
        let key = config.node_key().unwrap();
        assert_eq!(key.public().to_peer_id(), peer);
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
        // node.id is left: keygen does not replace the node.
        config.keygen().unwrap_err();

        // A key without the proof of work is refused.
        let mut config = NodeConfig::default();
        config.storage.dir = temp_dir();
        config.network.id_pow_bits = 0;
        config.validate().unwrap();
        config.keygen().unwrap();
        config.network.id_pow_bits = 32;
        config.node_key().unwrap_err();
    }
}
