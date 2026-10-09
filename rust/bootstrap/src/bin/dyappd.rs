//! `dyappd [--config <file>]`: checks the config, opens the stores and runs the libp2p node.
//! Any config problem stops the node before it opens a store or a socket.
//!
//! `dyappd keygen [--config <file>]`: makes the node key once (about a minute on 2 vCPU) and
//! prints the peer ID; the node does not start without it.
//!
//! `dyappd deny add <entry> [note]`, `deny remove <entry>`, `deny list`: edit the deny list in
//! `<storage.dir>/deny.db`; a running node applies a change within 10 seconds.
//! `dyappd deny received`: the lists other nodes shared, stored only.
//!
//! `dyappd status`: what the stores hold, read-only. Every subcommand runs as the owner of
//! `storage.dir`, so the node can still write the files it opens.

use dyapp_bootstrap::config::{transports, Role};
use dyapp_bootstrap::deny::{normalize, DenyStore};
use dyapp_bootstrap::media::MediaStore;
use dyapp_bootstrap::service::Service;
use dyapp_bootstrap::{node, BootstrapStore, NodeConfig};
use dyapp_p2p_net::proto;
use prost::Message;
use rusqlite::{Connection, OpenFlags};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: dyappd [keygen | status | deny add <entry> [note] | \
                     deny remove <entry> | deny list | deny received] [--config <file>]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let mut command: Vec<String> = std::env::args().skip(1).collect();
    let file = match command.iter().position(|a| a == "--config") {
        Some(at) if at + 1 < command.len() => {
            let path = command.remove(at + 1);
            command.remove(at);
            Some(PathBuf::from(path))
        }
        Some(_) => anyhow::bail!(USAGE),
        None => None,
    };
    let (config, ignored) = NodeConfig::load(file.as_deref(), std::env::vars())?;
    for key in ignored {
        tracing::warn!("unknown config key ignored: {key}");
    }
    config.validate()?;
    dyapp_p2p_net::set_id_pow_bits(config.network.id_pow_bits);
    dyapp_p2p_net::set_distinct_groups(config.network.distinct_outbound_groups);
    let command: Vec<&str> = command.iter().map(String::as_str).collect();
    if !command.is_empty() && command != ["keygen"] {
        let owner = std::fs::metadata(&config.storage.dir)?.uid();
        // SAFETY: geteuid has no preconditions.
        if owner != unsafe { libc::geteuid() } {
            anyhow::bail!(
                "run as the owner of {} (sudo -u dyappd dyappd ...)",
                config.storage.dir.display()
            );
        }
    }
    match command.as_slice() {
        [] => {}
        ["keygen"] => println!("{}", config.keygen()?),
        ["status"] => print!("{}", status(&config)),
        ["deny", rest @ ..] => deny(&config, rest)?,
        _ => anyhow::bail!(USAGE),
    }
    if !command.is_empty() {
        return Ok(());
    }
    let keypair = config.node_key()?;
    let deny_signer = config
        .network
        .share_deny_list
        .then(|| dyapp_bootstrap::deny::signer(&keypair))
        .flatten();
    let store = BootstrapStore::open(
        &config.storage.profiles_path(),
        &config.storage.messages_path(),
    )?;

    let mut swarm = node::swarm(keypair, &config.limits, &config.roles)?;
    // A host without IPv6 cannot open [::]; the node runs on whatever does open.
    let mut listening = 0;
    for address in &config.listen {
        for address in transports(address)? {
            match swarm.listen_on(address.clone()) {
                Ok(_) => listening += 1,
                Err(error) => tracing::warn!(%address, %error, "not listening"),
            }
        }
    }
    anyhow::ensure!(listening > 0, "listen: no address could be opened");
    for address in &config.external {
        for address in transports(address)? {
            swarm.add_external_address(address);
        }
    }
    // Anchors are dialled first, the cache fills the routing table, and seeds are dialled by the
    // first maintenance run of `node::run`.
    for anchor in node::cached_peers(&config.storage.anchors_path()) {
        if let Err(error) = swarm.dial(anchor.clone()) {
            tracing::warn!(%anchor, %error, "anchor not dialed");
        }
    }
    let cached = node::cached_peers(&config.storage.peers_path());
    dyapp_p2p_net::join(&mut swarm, &[], &cached);
    tracing::info!(peer_id = %swarm.local_peer_id(), roles = ?config.roles, "node started");
    let media = config.roles.contains(&Role::Media);
    let media = media
        .then(|| MediaStore::open(&config.storage.dir))
        .transpose()?;
    let mut service = Service::new(store, config);
    service.media = media;
    service.deny_signer = deny_signer;
    node::run(swarm, service).await;
    Ok(())
}

/// `dyappd deny ...`, with `deny` stripped from `args`.
fn deny(config: &NodeConfig, args: &[&str]) -> anyhow::Result<()> {
    let deny = DenyStore::open(&config.storage.deny_path())?;
    let entry = |entry: &str| {
        normalize(entry, &config.limits).ok_or_else(|| {
            anyhow::anyhow!("{entry}: not a peer ID, a SHA-256 or an IP group of the node's prefix")
        })
    };
    match args {
        ["add", e, note @ ..] => {
            let e = entry(e)?;
            let added = deny.add(&e, &note.join(" "))?;
            println!("{e} {}", if added { "added" } else { "already listed" });
        }
        ["remove", e] => {
            let e = entry(e)?;
            let removed = deny.remove(&e)?;
            println!("{e} {}", if removed { "removed" } else { "not listed" });
        }
        ["list"] => {
            for (e, note, added) in deny.list()? {
                let added = chrono::DateTime::from_timestamp(added, 0).unwrap_or_default();
                println!("{e}\t{}\t{note}", added.format("%Y-%m-%d %H:%M"));
            }
        }
        ["received"] => {
            for (peer, time, record) in deny.received()? {
                let time = chrono::DateTime::from_timestamp(time, 0).unwrap_or_default();
                let list = proto::DenyList::decode(record.payload.as_slice())?;
                println!("{peer}\t{}", time.format("%Y-%m-%d %H:%M"));
                for e in list.entries {
                    println!("\t{e}");
                }
            }
        }
        _ => anyhow::bail!(USAGE),
    }
    Ok(())
}

/// One `name value` line per count; a store not created yet shows `none`.
fn status(config: &NodeConfig) -> String {
    let s = &config.storage;
    let count = |path: &Path, sql: &str| {
        if !path.exists() {
            return "none".to_string();
        }
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .and_then(|db| db.query_row(sql, [], |row| row.get::<_, i64>(0)))
            .map_or_else(|e| format!("error: {e}"), |n| n.to_string())
    };
    let media = s.media_path();
    let peer_id = std::fs::read_to_string(s.dir.join("node.id")).unwrap_or_default();
    [
        ("peer_id", peer_id.trim().to_string()),
        (
            "profiles",
            count(&s.profiles_path(), "SELECT count(*) FROM profiles"),
        ),
        (
            "envelopes",
            count(&s.messages_path(), "SELECT count(*) FROM envelopes"),
        ),
        (
            "envelope_bytes",
            count(
                &s.messages_path(),
                "SELECT coalesce(sum(size), 0) FROM envelopes",
            ),
        ),
        (
            "media_blobs",
            count(
                &media,
                "SELECT count(DISTINCT hash) FROM blobs WHERE size IS NOT NULL",
            ),
        ),
        (
            "media_bytes",
            count(
                &media,
                "SELECT coalesce(sum(size), 0) FROM (SELECT DISTINCT hash, size FROM blobs)",
            ),
        ),
        (
            "deny_entries",
            count(&s.deny_path(), "SELECT count(*) FROM deny"),
        ),
        (
            "cached_peers",
            node::cached_peers(&s.peers_path()).len().to_string(),
        ),
    ]
    .iter()
    .map(|(name, value)| format!("{name} {value}\n"))
    .collect()
}
