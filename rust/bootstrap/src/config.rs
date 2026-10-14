//! Node configuration: defaults, then a TOML file, then `DYAPPD__<SECTION>__<KEY>` variables,
//! then `--<section>.<key> <value>` options.
//! Every field has a default, so a config written for an older node keeps working. Unknown keys
//! are logged and ignored, so a node rolled back to an older version still starts.

use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

pub const ENV_PREFIX: &str = "DYAPPD__";

/// The packaged `/etc/dyappd.toml`: every key commented out with its default and a description.
pub const REFERENCE: &str = include_str!("../dyappd.toml");

/// The key lines of [`REFERENCE`]: `# key = default  # description` as (`section.`, key,
/// default, description).
fn documented() -> Vec<(String, &'static str, &'static str, &'static str)> {
    let mut section = String::new();
    let mut keys = Vec::new();
    for line in REFERENCE.lines() {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = format!("{name}.");
        } else if let Some((key, rest)) = line
            .strip_prefix("# ")
            .and_then(|l| l.split_once(" = "))
            .filter(|(key, _)| {
                key.bytes()
                    .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
            })
        {
            let (default, about) = rest.split_once("  # ").unwrap_or((rest, ""));
            keys.push((section.clone(), key, default.trim_end(), about));
        }
    }
    keys
}

/// The options of `dyappd --help`, one per config key, with its default and description.
pub fn options_help() -> String {
    let flags: Vec<_> = documented()
        .into_iter()
        .map(|(section, key, default, about)| {
            let flag = format!("--{section}{}", key.replace('_', "-"));
            (flag, default, about)
        })
        .collect();
    let width = flags.iter().map(|(flag, ..)| flag.len()).max().unwrap_or(0);
    flags
        .iter()
        .map(|(flag, default, about)| {
            format!("  {flag:<width$}  {default}  {about}")
                .trim_end()
                .to_owned()
                + "\n"
        })
        .collect()
}

/// A TOML value, or the text as a string when it is not one (a bare path, for example).
fn toml_value(text: &str) -> toml::Value {
    format!("v = {text}")
        .parse::<toml::Table>()
        .ok()
        .and_then(|mut t| t.remove("v"))
        .unwrap_or_else(|| toml::Value::String(text.to_owned()))
}

/// Sets `path` in `table`, creating the sections on the way; `name` labels errors.
fn set(
    table: &mut toml::Table,
    path: &[String],
    value: toml::Value,
    name: &str,
) -> anyhow::Result<()> {
    let (last, sections) = path.split_last().expect("split yields at least one item");
    let mut node = table;
    for section in sections {
        node = node
            .entry(section.clone())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("{name}: {section} is not a section"))?;
    }
    node.insert(last.clone(), value);
    Ok(())
}

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

/// TCP and QUIC addresses of a `host:port[/peer-ID]` seed, with an IP or DNS host.
pub fn seed(address: &str) -> anyhow::Result<[Multiaddr; 2]> {
    if let Some((host, peer)) = address.split_once('/') {
        let peer: PeerId = peer.parse().map_err(|_| {
            anyhow::anyhow!("bad seed {address}: invalid peer ID; expected host:port/<peer ID>")
        })?;
        return Ok(seed(host)?.map(|address| address.with(Protocol::P2p(peer))));
    }
    if let Ok(addresses) = transports(address) {
        return Ok(addresses);
    }
    let (host, port) = address
        .rsplit_once(':')
        .filter(|(host, _)| !host.is_empty() && !host.contains([':', '/', '[']))
        .and_then(|(host, port)| Some((host, port.parse::<u16>().ok()?)))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "bad seed {address}: expected host:port, e.g. seed.example.org:7070 or \
                 [2001:db8::7]:7070"
            )
        })?;
    let dns = Multiaddr::empty().with(Protocol::Dns(host.into()));
    Ok([
        dns.clone().with(Protocol::Tcp(port)),
        dns.with(Protocol::Udp(port)).with(Protocol::QuicV1),
    ])
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct NodeConfig {
    /// `host:port` addresses, each served over TCP and QUIC; see [`transports`].
    pub listen: Vec<String>,
    /// `host:port` addresses announced to peers when the operator knows them; AutoNAT confirms
    /// others.
    pub external: Vec<String>,
    /// `host:port[/peer-ID]` nodes dialed over TCP and QUIC; see [`seed`].
    pub seeds: Vec<String>,
    pub roles: Vec<Role>,
    pub storage: StorageConfig,
    pub limits: Limits,
    pub maintenance: Maintenance,
    pub network: Network,
    pub turn: Turn,
    pub media: Media,
    pub search: Search,
}

/// The search role, see docs/architecture/bootstrap.md#search.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Search {
    /// Largest profile record indexed, in bytes; at most 1 MiB, reported in `info`.
    pub max_profile_bytes: usize,
    /// Bytes of `profiles-idx.db`: records plus index; above it the oldest heartbeats leave.
    pub max_bytes: u64,
    /// Rows one query scans at most; past it the reply is `partial`.
    pub max_scan_rows: u32,
    /// Gossip topics (0..16) this node keeps; empty = all.
    pub topics: Vec<u32>,
    /// Leading zero bits of the Argon2id proof of work a profile needs to be indexed; at most
    /// 20, reported in `info`.
    pub min_profile_pow_bits: u32,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            max_profile_bytes: 64 << 10,
            max_bytes: 50 << 30,
            max_scan_rows: 10_000,
            topics: vec![],
            min_profile_pow_bits: 11,
        }
    }
}

impl Search {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.max_profile_bytes == 0
            || self.max_profile_bytes > crate::service::SEARCH_ACCEPT_BYTES
        {
            anyhow::bail!(
                "search.max_profile_bytes must be between 1 and {} bytes",
                crate::service::SEARCH_ACCEPT_BYTES
            );
        }
        if self.max_scan_rows == 0 || self.max_bytes == 0 {
            anyhow::bail!("search.max_scan_rows and max_bytes must be at least 1");
        }
        if self.topics.iter().any(|t| *t >= 16) {
            anyhow::bail!("search.topics must be in 0..16");
        }
        if self.min_profile_pow_bits > 20 {
            anyhow::bail!("search.min_profile_pow_bits must be at most 20");
        }
        Ok(())
    }

    /// Whether this node keeps keys of `topic`.
    pub fn keeps(&self, topic: u32) -> bool {
        self.topics.is_empty() || self.topics.contains(&topic)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Media {
    /// Whole-copy threshold in bytes; stored objects never exceed 1 MiB.
    pub shard_threshold: usize,
}

impl Default for Media {
    fn default() -> Self {
        Self {
            shard_threshold: 1 << 20,
        }
    }
}

/// The coturn relay next to the node (role turn), run with `use-auth-secret`.
#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Network {
    /// Exact network name; empty selects public. Binary activation awaits storage binding.
    pub name: String,
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
            name: String::new(),
            id_pow_bits: dyapp_p2p_net::ID_POW_BITS,
            distinct_outbound_groups: true,
            storage_trust_minutes: 60,
            share_deny_list: false,
            accept_deny_lists: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Store,
    Media,
    Search,
    Turn,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Holds the node key and every database; media blob files go in `data/`.
    pub dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    /// Media requests per second from one peer: a reply carries up to 6 MiB.
    pub media_requests_per_second: u32,
    /// Free space kept on the file system of `storage.dir`.
    pub min_free_mb: u64,
    /// Node protocol bytes in and out per second: media and repair are shed from 75%, profiles
    /// from 90%, the mailbox at 100%; 0 = no limit.
    pub bytes_per_second: u64,
    pub max_connections: u32,
    pub max_connections_per_peer: u32,
    /// Pending and established incoming connections per fine IP group; outgoing exempt.
    pub max_connections_per_ip_group: u32,
    /// Concurrent streams per connection and protocol.
    pub max_streams: usize,
    /// Physical memory of the process: from 80 % of it requests that shed by the byte rate are
    /// refused, at it every request is dropped and new connections refused; 0 = no limit.
    pub max_memory_mb: u64,
    /// Requests per second shared by all peers of one IP group: IPv4 and IPv6 addresses with
    /// the same leading `ipv4_prefix` / `ipv6_prefix` bits.
    pub ip_group_requests_per_second: u32,
    pub ipv4_prefix: u8,
    pub ipv6_prefix: u8,
    /// Planned: a second group budget at ten times `ip_group_requests_per_second`.
    pub coarse_ipv4_prefix: u8,
    pub coarse_ipv6_prefix: u8,
    /// Envelopes per second accepted from one sender key.
    pub sender_puts_per_second: u32,
    /// Refused floods and bad signatures that ban a peer for `ban_minutes`.
    pub strikes_to_ban: u32,
    pub ban_minutes: u32,
}

/// Store maintenance: see `BootstrapStore::maintain`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Maintenance {
    pub interval_minutes: u32,
    /// Free pages released per store and run (4 KiB each): bounds the I/O of one run.
    pub vacuum_pages: u32,
    /// Shared background rebuild write budget, in MiB per second.
    pub io_mb_per_s: u32,
}

impl Default for Maintenance {
    fn default() -> Self {
        Self {
            interval_minutes: 60,
            vacuum_pages: 2048,
            io_mb_per_s: 16,
        }
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            // Loopback until the operator opens the node with ["[::]:7070", "0.0.0.0:7070"];
            // libp2p binds IPv6 sockets v6-only, so dual stack takes both addresses.
            listen: vec!["[::1]:7070".into(), "127.0.0.1:7070".into()],
            external: vec![],
            seeds: vec![],
            roles: vec![Role::Store],
            storage: StorageConfig::default(),
            limits: Limits::default(),
            maintenance: Maintenance::default(),
            network: Network::default(),
            turn: Turn::default(),
            media: Media::default(),
            search: Search::default(),
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            dir: "/var/lib/dyappd".into(),
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
            bytes_per_second: 0,
            max_connections: 1000,
            max_connections_per_peer: 4,
            max_connections_per_ip_group: 32,
            max_streams: 16,
            max_memory_mb: 768,
            ip_group_requests_per_second: 1000,
            ipv4_prefix: 24,
            ipv6_prefix: 48,
            coarse_ipv4_prefix: 16,
            coarse_ipv6_prefix: 32,
            sender_puts_per_second: 10,
            strikes_to_ban: 100,
            ban_minutes: 10,
        }
    }
}

impl StorageConfig {
    pub fn profiles_path(&self) -> PathBuf {
        self.dir.join("profiles.db")
    }

    /// The search role's index, `profiles-idx.db`.
    pub fn search_path(&self) -> PathBuf {
        self.dir.join("profiles-idx.db")
    }

    pub fn messages_path(&self) -> PathBuf {
        self.dir.join("messages.db")
    }

    /// `media.db`; the blob files are in `data/` next to it, see [`crate::media::MediaStore`].
    pub fn media_path(&self) -> PathBuf {
        self.dir.join("media.db")
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

    fn stores(&self) -> [PathBuf; 4] {
        [
            self.profiles_path(),
            self.search_path(),
            self.messages_path(),
            self.media_path(),
        ]
    }
}

impl NodeConfig {
    /// Reads the file (if given), the environment and the `--` options as (`section.key`,
    /// value); an option is named as its key with `-` for `_`, and a list option is given once
    /// per item. Returns the config and the ignored keys; an unknown option is refused.
    pub fn load(
        file: Option<&Path>,
        env: impl IntoIterator<Item = (String, String)>,
        options: &[(String, String)],
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
            set(&mut table, &path, toml_value(&value), &name)?;
        }
        let defaults = toml::Value::try_from(Self::default())?;
        let mut lists: Vec<(Vec<String>, Vec<toml::Value>)> = Vec::new();
        for (option, text) in options {
            let path: Vec<String> = option.split('.').map(|k| k.replace('-', "_")).collect();
            let default = path
                .iter()
                .try_fold(&defaults, |node, key| node.get(key.as_str()))
                .filter(|node| !node.is_table())
                .ok_or_else(|| anyhow::anyhow!("unknown option --{option}, see dyappd --help"))?;
            // Text options are never parsed: a secret of digits stays a string.
            let value = match default {
                toml::Value::String(_) | toml::Value::Array(_) => toml::Value::String(text.clone()),
                _ => toml_value(text),
            };
            if !default.is_array() {
                set(&mut table, &path, value, option)?;
            } else if let Some((_, items)) = lists.iter_mut().find(|(p, _)| *p == path) {
                items.push(value);
            } else {
                lists.push((path, vec![value]));
            }
        }
        for (path, items) in lists {
            set(&mut table, &path, toml::Value::Array(items), "option")?;
        }
        let mut ignored = Vec::new();
        let config = serde_ignored::deserialize(table, |path| ignored.push(path.to_string()))
            .map_err(|e| anyhow::anyhow!("config: {e}"))?;
        // Ignoring a removed store path would start that store empty in storage.dir.
        if let Some(key) = ignored.iter().find(|key| {
            ["storage.profiles", "storage.messages", "storage.media"].contains(&key.as_str())
        }) {
            anyhow::bail!("{key} is removed: move the store into storage.dir and drop the key");
        }
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
            seed(address)?;
        }
        if self.listen.is_empty() {
            anyhow::bail!("listen: at least one address is required");
        }
        if let Some(role) = self
            .roles
            .iter()
            .find(|r| !matches!(r, Role::Store | Role::Media | Role::Turn | Role::Search))
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
        if self.roles.contains(&Role::Search) {
            // As with turn: a client-mode node serves no protocol.
            if !self.roles.contains(&Role::Store) {
                anyhow::bail!("role search needs role store");
            }
            self.search.validate()?;
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
        if self.media.shard_threshold == 0 || self.media.shard_threshold > 1 << 20 {
            anyhow::bail!("media.shard_threshold must be between 1 and 1048576 bytes");
        }
        let l = &self.limits;
        if l.max_connections < 1
            || l.max_connections_per_peer < 1
            || l.max_connections_per_ip_group < 1
            || l.max_streams < 1
        {
            anyhow::bail!("limits.max_connections, max_connections_per_peer, max_connections_per_ip_group and max_streams must be at least 1");
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
        if l.coarse_ipv4_prefix > 32 || l.coarse_ipv6_prefix > 128 {
            anyhow::bail!(
                "limits.coarse_ipv4_prefix must be at most 32 and coarse_ipv6_prefix at most 128"
            );
        }
        if l.ip_group_requests_per_second.checked_mul(10).is_none() {
            anyhow::bail!("limits.ip_group_requests_per_second must be at most 429496729 for the 10x coarse budget");
        }
        if self.network.id_pow_bits > 32 {
            anyhow::bail!("network.id_pow_bits must be at most 32");
        }
        if self.maintenance.interval_minutes < 1
            || self.maintenance.vacuum_pages < 1
            || self.maintenance.io_mb_per_s < 1
        {
            anyhow::bail!(
                "maintenance.interval_minutes, vacuum_pages and io_mb_per_s must be at least 1"
            );
        }
        dyapp_p2p_net::Protocols::new(&self.network.name)?;
        check_writable(&self.storage.dir)
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
        let encoded = Zeroizing::new(keypair.to_protobuf_encoding()?);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&key_path)
            .map_err(|e| anyhow::anyhow!("{}: {e}", key_path.display()))?
            .write_all(&encoded)?;
        let peer_id = keypair.public().to_peer_id();
        fs::write(&id_path, peer_id.to_string())?;
        Ok(peer_id)
    }

    /// Loads the node key made by [`Self::keygen`]. A key that does not belong to the stored data
    /// or lacks the proof of work is refused, as is a key with group/other permission bits.
    pub fn node_key(&self) -> anyhow::Result<Keypair> {
        let key_path = self.storage.key_path();
        let id_path = self.storage.dir.join("node.id");
        let keypair = {
            let mut file = match File::open(&key_path) {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => anyhow::bail!(
                    "{} is missing: run `dyappd keygen` once and keep the key safe",
                    key_path.display()
                ),
                Err(e) => return Err(anyhow::anyhow!("{}: {e}", key_path.display())),
            };
            // Check the opened file, not a path that could be replaced before reading.
            let metadata = file
                .metadata()
                .map_err(|e| anyhow::anyhow!("{}: {e}", key_path.display()))?;
            if metadata.mode() & 0o077 != 0 {
                anyhow::bail!(
                    "{} has group/other permissions: use chmod 600 before starting",
                    key_path.display()
                );
            }
            // Read into one fixed allocation: growing a Vec could leave freed secret copies.
            let mut bytes = Zeroizing::new(vec![0; usize::try_from(metadata.len())?]);
            file.read_exact(&mut bytes)
                .map_err(|e| anyhow::anyhow!("{}: {e}", key_path.display()))?;
            let mut extra = Zeroizing::new([0u8; 1]);
            if file
                .read(&mut extra[..])
                .map_err(|e| anyhow::anyhow!("{}: {e}", key_path.display()))?
                != 0
            {
                anyhow::bail!("{} grew while reading; retry startup", key_path.display());
            }
            Keypair::from_protobuf_encoding(&bytes)
                .map_err(|e| anyhow::anyhow!("{}: {e}", key_path.display()))?
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
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 64,
            failure_persistence: Some(Box::new(
                proptest::test_runner::FileFailurePersistence::Direct(
                    "/tmp/ai/dyapp-config-proptest-regressions.txt"
                )
            )),
            ..ProptestConfig::default()
        })]

        #[test]
        fn prop_set_preserves_values_and_unrelated_fields(
            path in proptest::collection::vec("[a-z]{1,8}", 1..5),
            value in any::<i64>(),
        ) {
            let mut table = toml::Table::new();
            table.insert("__sentinel".into(), toml::Value::Boolean(true));
            set(&mut table, &path, toml::Value::Integer(value), "generated").unwrap();
            let root = toml::Value::Table(table);
            let actual = path.iter().try_fold(&root, |node, key| node.get(key));
            prop_assert_eq!(actual, Some(&toml::Value::Integer(value)));
            prop_assert_eq!(root.get("__sentinel"), Some(&toml::Value::Boolean(true)));
        }

        #[test]
        fn prop_config_precedence(
            file_value in any::<u32>(),
            env_value in any::<u32>(),
            cli_value in any::<u32>(),
            use_env in any::<bool>(),
            use_cli in any::<bool>(),
        ) {
            let dir = temp_dir();
            fs::create_dir_all(&dir)?;
            let file = dir.join("config.toml");
            fs::write(&file, format!("[limits]\nrequests_per_second = {file_value}\n"))?;
            let env = if use_env {
                vec![("DYAPPD__LIMITS__REQUESTS_PER_SECOND".into(), env_value.to_string())]
            } else {
                vec![]
            };
            let options = if use_cli {
                vec![("limits.requests-per-second".into(), cli_value.to_string())]
            } else {
                vec![]
            };
            let (config, ignored) = NodeConfig::load(Some(&file), env, &options).unwrap();
            let expected = if use_cli { cli_value } else if use_env { env_value } else { file_value };
            prop_assert_eq!(config.limits.requests_per_second, expected);
            prop_assert!(ignored.is_empty());
        }

        #[test]
        fn prop_cli_strings_remain_literal(
            text in prop_oneof![Just("00123".to_owned()), Just("a=b,c=d".to_owned()), ".{0,80}"],
        ) {
            let (config, _) = NodeConfig::load(
                None,
                Vec::<(String, String)>::new(),
                &[("turn.secret".into(), text.clone())],
            ).unwrap();
            prop_assert_eq!(config.turn.secret, text);
        }

        #[test]
        fn prop_section_collisions_and_unknown_options_are_errors(
            key in "[a-z]{1,12}",
            value in any::<i64>(),
        ) {
            let mut table = toml::Table::new();
            table.insert("section".into(), toml::Value::Integer(value));
            let error = set(&mut table, &["section".into(), key.clone()], toml::Value::Integer(1), "generated")
                .unwrap_err().to_string();
            prop_assert!(error.contains("section is not a section"));
            let option = format!("unknown-{key}");
            let error = NodeConfig::load(
                None, Vec::<(String, String)>::new(), &[(option.clone(), value.to_string())]
            ).unwrap_err().to_string();
            let expected = format!("unknown option --{option}");
            prop_assert!(error.contains(&expected));
        }

        #[test]
        fn prop_media_threshold_validates_documented_range(
            threshold in prop_oneof![
                Just(0usize), Just(1usize), Just(1usize << 20),
                Just((1usize << 20) + 1), Just(usize::MAX), 0usize..=(2usize << 20)
            ],
        ) {
            let mut config = NodeConfig::default();
            config.storage.dir = temp_dir();
            config.media.shard_threshold = threshold;
            prop_assert_eq!(config.validate().is_ok(), (1..=1048576).contains(&threshold));
        }
    }

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
        let (config, ignored) = NodeConfig::load(Some(&file), env, &[]).unwrap();
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
    fn options_override_env() {
        let env = [("DYAPPD__LIMITS__MAX_CONNECTIONS".into(), "5".into())];
        let options = [
            ("limits.max-connections", "7"),
            ("listen", "[::]:7070"),
            ("listen", "0.0.0.0:7070"),
            ("turn.secret", "123"),
            ("network.share-deny-list", "true"),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()));
        let (config, _) = NodeConfig::load(None, env, &options).unwrap();
        assert_eq!(config.limits.max_connections, 7);
        assert_eq!(config.listen, ["[::]:7070", "0.0.0.0:7070"]);
        assert_eq!(config.turn.secret, "123");
        assert!(config.network.share_deny_list);
        for option in ["limits.max-connection", "limits", "x"] {
            let options = [(option.to_owned(), "1".to_owned())];
            assert!(NodeConfig::load(None, [], &options).is_err());
        }
    }

    /// The packaged file, and so `--help`, lists every key with the default the code uses.
    #[test]
    fn reference_matches_defaults() {
        let mut table = toml::Table::new();
        for (section, key, default, _) in documented() {
            let path: Vec<String> = format!("{section}{key}")
                .split('.')
                .map(Into::into)
                .collect();
            set(&mut table, &path, toml_value(default), key).unwrap();
        }
        let defaults = toml::Value::try_from(NodeConfig::default()).unwrap();
        assert_eq!(toml::Value::Table(table), defaults);
        assert!(options_help().contains("--limits.max-connections"));
    }

    #[test]
    fn network_names_validate_without_normalization_and_load_as_literal_strings() {
        let mut config = NodeConfig::default();
        config.storage.dir = temp_dir();
        assert!(config.network.name.is_empty());
        for name in ["", "City", "city", "a b", "00123", &"a".repeat(64)] {
            config.network.name = name.into();
            config.validate().unwrap();
            assert_eq!(config.network.name, name);
        }
        for name in [" a", "a ", "a\nb", "сеть", &"a".repeat(65)] {
            config.network.name = name.into();
            let expected = dyapp_p2p_net::Protocols::new(name).unwrap_err().to_string();
            assert_eq!(config.validate().unwrap_err().to_string(), expected);
        }
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("network.toml");
        fs::write(&path, "[network]\nname = \"file\"\n").unwrap();
        let load = |env: Vec<(String, String)>, options: Vec<(String, String)>| {
            let (config, ignored) = NodeConfig::load(Some(&path), env, &options).unwrap();
            assert!(ignored.is_empty());
            config.network.name
        };
        assert_eq!(load(vec![], vec![]), "file");
        assert_eq!(
            load(vec![("DYAPPD__NETWORK__NAME".into(), "env".into())], vec![]),
            "env"
        );
        assert_eq!(
            load(
                vec![("DYAPPD__NETWORK__NAME".into(), "env".into())],
                vec![("network.name".into(), "00123".into())]
            ),
            "00123"
        );
        assert!(options_help().contains("--network.name"));
    }

    #[test]
    fn search_limits_are_checked() {
        let search = |change: fn(&mut Search)| {
            let mut config = NodeConfig::default();
            change(&mut config.search);
            config.search.validate()
        };
        assert!(search(|_| {}).is_ok());
        assert!(search(|s| s.max_profile_bytes = 0).is_err());
        assert!(search(|s| s.max_profile_bytes = crate::service::SEARCH_ACCEPT_BYTES + 1).is_err());
        assert!(search(|s| s.max_profile_bytes = crate::service::SEARCH_ACCEPT_BYTES).is_ok());
        assert!(search(|s| s.max_scan_rows = 0).is_err());
        assert!(search(|s| s.topics = vec![16]).is_err());
        assert!(search(|s| s.max_bytes = 0).is_err());
        assert_eq!(Search::default().max_profile_bytes, 64 << 10);
        assert_eq!(Search::default().min_profile_pow_bits, 11);
        assert!(search(|s| s.min_profile_pow_bits = 20).is_ok());
        assert!(search(|s| s.min_profile_pow_bits = 21).is_err());
        assert!(Search::default().keeps(15));
        let one = Search {
            topics: vec![3],
            ..Search::default()
        };
        assert!(one.keeps(3) && !one.keeps(4));
    }

    #[test]
    fn incoming_group_and_coarse_settings_validate_boundaries() {
        let mut config = NodeConfig::default();
        config.storage.dir = temp_dir();
        assert_eq!(config.limits.max_connections_per_ip_group, 32);
        assert_eq!(config.limits.coarse_ipv4_prefix, 16);
        assert_eq!(config.limits.coarse_ipv6_prefix, 32);
        assert!(config.validate().is_ok());

        config.limits.max_connections_per_ip_group = 0;
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("max_connections_per_ip_group"));
        config.limits.max_connections_per_ip_group = 1;
        for (v4, v6) in [(0, 0), (32, 128), (16, 32)] {
            config.limits.coarse_ipv4_prefix = v4;
            config.limits.coarse_ipv6_prefix = v6;
            // Coarse masks are independent: original endpoint metadata is needed at runtime.
            config.limits.ipv4_prefix = 0;
            config.limits.ipv6_prefix = 0;
            assert!(config.validate().is_ok());
        }
        config.limits.coarse_ipv4_prefix = 33;
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("coarse_ipv4_prefix"));
        config.limits.coarse_ipv4_prefix = 16;
        config.limits.coarse_ipv6_prefix = 129;
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("coarse_ipv6_prefix"));
        config.limits.coarse_ipv6_prefix = 32;
        for rate in [1, u32::MAX / 10] {
            config.limits.ip_group_requests_per_second = rate;
            assert!(config.validate().is_ok());
        }
        for rate in [0, u32::MAX / 10 + 1, u32::MAX] {
            config.limits.ip_group_requests_per_second = rate;
            assert!(config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("ip_group_requests_per_second"));
        }
    }

    #[test]
    fn seeds_can_pin_ipv4_ipv6_and_dns_without_accepting_multiaddrs() {
        let peer = PeerId::random();
        for host in ["127.0.0.1:7070", "[2001:db8::7]:7070", "seed.example:7070"] {
            let plain = seed(host).unwrap();
            let pinned = seed(&format!("{host}/{peer}")).unwrap();
            assert_eq!(
                pinned,
                plain.map(|address| address.with(Protocol::P2p(peer)))
            );
        }
        for bad in [
            "seed.example:7070/",
            "seed.example:7070/not-a-peer-id",
            "/dns/seed.example/tcp/7070",
            "/ip4/127.0.0.1/tcp/7070",
        ] {
            assert!(seed(bad).is_err(), "{bad}");
        }
        assert!(seed(&format!("seed.example:7070/{peer}/extra")).is_err());
    }

    #[tokio::test]
    async fn seed_pins_refuse_the_wrong_transport_key_and_plain_seeds_still_connect() {
        use libp2p::futures::StreamExt;
        use libp2p::identity::Keypair;
        use libp2p::swarm::{DialError, SwarmEvent};
        use std::time::Duration;
        let mut server =
            dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), dyapp_p2p_net::Mode::Auto)
                .unwrap();
        let real = *server.local_peer_id();
        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let address = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address;
            }
        };
        let port = address
            .iter()
            .find_map(|part| match part {
                Protocol::Tcp(port) => Some(port),
                _ => None,
            })
            .unwrap();
        for expected in [None, Some(real), Some(PeerId::random())] {
            let mut client = dyapp_p2p_net::build_swarm(
                Keypair::generate_ed25519(),
                dyapp_p2p_net::Mode::Client,
            )
            .unwrap();
            let host = format!("127.0.0.1:{port}");
            let text = expected.map_or(host.clone(), |peer| format!("{host}/{peer}"));
            let seeds = seed(&text).unwrap();
            dyapp_p2p_net::join(&mut client, &seeds, &[]);
            let wrong = expected.is_some_and(|id| id != real);
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    tokio::select! {
                        _ = server.select_next_some() => {}
                        event = client.select_next_some() => match event {
                            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                                assert!(!wrong, "connected to a different seed key");
                                assert_eq!(peer_id, real);
                                break;
                            }
                            SwarmEvent::OutgoingConnectionError {
                                error: DialError::WrongPeerId { .. }, ..
                            } if wrong => {
                                assert_eq!(client.connected_peers().count(), 0);
                                assert!(client.behaviour_mut().kad.kbuckets().all(|bucket| bucket.num_entries() == 0));
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }).await.expect("seed authentication never resolved");
        }
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
        assert!(invalid(|c| c.media.shard_threshold = (1 << 20) + 1));
        assert!(invalid(|c| c.media.shard_threshold = 0));
        assert!(!invalid(|c| c.media.shard_threshold = 1));
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
        assert!(invalid(
            |c| c.seeds = vec!["seed.example:7070/not-a-peer-id".into()]
        ));
        assert!(invalid(
            |c| c.seeds = vec!["/dns/seed.example/tcp/7070".into()]
        ));
        assert!(invalid(|c| c.seeds = vec!["[seed.example]:7070".into()]));
        assert_eq!(
            seed("seed.example:7070").unwrap().map(|a| a.to_string()),
            [
                "/dns/seed.example/tcp/7070",
                "/dns/seed.example/udp/7070/quic-v1"
            ]
        );
        assert_eq!(
            seed("[2001:db8::7]:7070").unwrap(),
            transports("[2001:db8::7]:7070").unwrap()
        );
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
        assert!(invalid(|c| c.roles = vec![Role::Search]));
        assert!(!invalid(|c| c.roles = vec![Role::Store, Role::Search]));
        assert!(invalid(|c| c.limits.message_ttl_hours = 0));
        assert!(invalid(|c| c.limits.attachment_retention_hours = 0));
        assert!(invalid(|c| c.limits.profile_ttl_days = 0));
        assert!(invalid(|c| c.maintenance.io_mb_per_s = 0));
        assert!(invalid(|c| c.storage.dir = "/proc/dyappd".into()));
        let roles = [("DYAPPD__ROLES".into(), "[\"x\"]".into())];
        assert!(NodeConfig::load(None, roles, &[]).is_err());
        let removed = [("DYAPPD__STORAGE__MEDIA".into(), "/big/media".into())];
        assert!(NodeConfig::load(None, removed, &[]).is_err());
    }

    #[test]
    fn node_key_refuses_group_other_bits_without_repairing_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let mut config = NodeConfig::default();
        config.storage.dir = temp_dir();
        config.network.id_pow_bits = 4;
        config.validate().unwrap();
        let peer = config.keygen().unwrap();
        let path = config.storage.key_path();
        for mode in [0o640, 0o604, 0o601] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            let error = config.node_key().unwrap_err().to_string();
            assert!(error.contains("chmod 600"), "{error}");
            assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, mode);
        }
        for mode in [0o600, 0o400] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(config.node_key().unwrap().public().to_peer_id(), peer);
            assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, mode);
        }
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
