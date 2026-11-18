//! `dyappd [--config <file>] [--<section>.<key> <value>]...`: checks the config, opens the stores
//! and runs the libp2p node. Without `--config` the first of `./dyappd.toml`, `~/dyappd.toml`
//! and `/etc/dyappd.toml` that exists is read. Any config problem stops the node before it opens
//! a store or a socket. `dyappd --help` lists the options, one per config key.
//!
//! `dyappd keygen [-] [--config <file>]`: makes the node key once (about a minute on 2 vCPU) and
//! prints the peer ID; the node does not start without it. With `-` the key goes to stdout, for
//! `systemd-creds encrypt`, and the peer ID to stderr.
//!
//! `dyappd deny add <entry> [note]`, `deny remove <entry>`, `deny list`: edit the deny list in
//! `<storage.dir>/deny.db`; a running node applies a change within 10 seconds.
//! `dyappd deny received`: the lists other nodes shared, stored only.
//!
//! `dyappd id`: the peer ID, then `<external address>/<peer ID>` seed entries for other nodes.
//! `dyappd probe-key`: read-only key/peer-ID check for a stopped backup copy; no listeners.
//!
//! `dyappd status`: whether the node answers, what the stores hold (read-only) and the traffic
//! totals from the metrics listener; exits 3 when the node does not answer. These subcommands
//! run as the owner of `storage.dir`, so the node can still write the files it opens.
//!
//! `dyappd info <host:port>` and the other [client] commands talk to a running node; they only
//! read the config, for `network.name`.

use dyapp_bootstrap::clock::Clock;
use dyapp_bootstrap::config::{message, options_help, options_help_russian, russian_locale};
use dyapp_bootstrap::deny::{normalize, DenyStore};
use dyapp_bootstrap::{node, NodeConfig};
use dyapp_p2p_net::proto;
use prost::Message;
use rusqlite::{Connection, OpenFlags};
use std::io::IsTerminal;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

mod client;

const USAGE: &str =
    "usage: dyappd [keygen [-] | id | probe-key | status | deny add <entry> [note] | \
                     deny remove <entry> | deny list | deny received] [--config <file>] \
                     [--<option> <value>]...";
const USAGE_RU: &str =
    "использование: dyappd [keygen [-] | id | probe-key | status | deny add <запись> [примечание] | \
                     deny remove <запись> | deny list | deny received] [--config <файл>] \
                     [--<параметр> <значение>]...";

fn usage() -> (&'static str, &'static str) {
    if russian_locale() {
        (USAGE_RU, client::USAGE_RU)
    } else {
        (USAGE, client::USAGE)
    }
}

fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> anyhow::Result<()> {
    let Some(arguments) = parse_args()? else {
        return Ok(());
    };
    let Arguments {
        file,
        options,
        command,
    } = arguments;
    init_tracing(&command);
    let file = file.or_else(default_file);
    let (config, ignored) = NodeConfig::load(file.as_deref(), std::env::vars(), &options)?;
    for key in ignored {
        tracing::warn!("unknown config key ignored: {key}");
    }
    let command: Vec<&str> = command.iter().map(String::as_str).collect();
    execute(config, file, &command).await
}

async fn execute(
    mut config: NodeConfig,
    file: Option<PathBuf>,
    command: &[&str],
) -> anyhow::Result<()> {
    if run_subcommand(&mut config, command).await? {
        return Ok(());
    }
    tracing::info!(file = ?file, "config");
    node::open(config).await?.run().await;
    Ok(())
}

fn init_tracing(command: &[String]) {
    // A subcommand prints its result to stdout, so its logs go to stderr.
    if command.is_empty() {
        tracing_subscriber::fmt::init();
    } else {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .init();
    }
}

async fn run_subcommand(config: &mut NodeConfig, command: &[&str]) -> anyhow::Result<bool> {
    if let Some(output) = client::run(&config.network.name, command).await? {
        println!("{output}");
        return Ok(true);
    }
    configure_node(config, command)?;
    if command.is_empty() {
        return Ok(false);
    }
    run_command(config, command).await?;
    Ok(true)
}

struct Arguments {
    file: Option<PathBuf>,
    options: Vec<(String, String)>,
    command: Vec<String>,
}

fn print_help() {
    let (usage, client_usage) = usage();
    if russian_locale() {
        print!(
            "{usage}\n{client_usage}\n\nБез --config читается первый существующий файл: \
             ./dyappd.toml, ~/dyappd.toml или /etc/dyappd.toml. Параметры командной строки \
             переопределяют файл и переменные DYAPPD_; для каждого элемента списка укажите \
             параметр отдельно. Значения по умолчанию:\n{}",
            options_help_russian()
        );
    } else {
        print!(
            "{usage}\n{client_usage}\n\nWithout --config the first of ./dyappd.toml, ~/dyappd.toml and \
             /etc/dyappd.toml that exists is read. Options override the config file and \
             DYAPPD_ variables; a list option is given once per item. Defaults:\n{}",
            options_help()
        );
    }
}

fn parse_args() -> anyhow::Result<Option<Arguments>> {
    let (mut file, mut options, mut command) = (None, Vec::new(), Vec::new());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-h" || arg == "--help" {
            print_help();
            return Ok(None);
        }
        let Some(option) = arg.strip_prefix("--") else {
            command.push(arg);
            continue;
        };
        let (key, value) = match option.split_once('=') {
            Some((key, value)) => (key.to_owned(), value.to_owned()),
            None => match args.next() {
                Some(value) => (option.to_owned(), value),
                None => anyhow::bail!(
                    "--{option} {}\n{}",
                    message("needs a value", "требует значения"),
                    usage().0
                ),
            },
        };
        if key == "config" {
            file = Some(PathBuf::from(value));
        } else {
            options.push((key, value));
        }
    }
    Ok(Some(Arguments {
        file,
        options,
        command,
    }))
}

fn configure_node(config: &mut NodeConfig, command: &[&str]) -> anyhow::Result<()> {
    config.resolve_secrets()?;
    config.validate()?;
    // Keeps the node key out of core dumps and away from ptrace by the user's other processes.
    #[cfg(target_os = "linux")]
    // SAFETY: PR_SET_DUMPABLE takes integers and touches no memory of the process.
    if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    dyapp_p2p_net::set_id_pow_bits(config.network.id_pow_bits);
    dyapp_p2p_net::set_distinct_groups(config.network.distinct_outbound_groups);
    if command.first().is_some_and(|&c| c != "keygen") {
        let owner = std::fs::metadata(&config.storage.dir)?.uid();
        // SAFETY: geteuid has no preconditions.
        if owner != unsafe { libc::geteuid() } {
            anyhow::bail!(
                "{} {} (sudo -u dyappd dyappd ...)",
                message("run as the owner of", "запустите от имени владельца"),
                config.storage.dir.display()
            );
        }
    }
    Ok(())
}

async fn run_command(config: &NodeConfig, command: &[&str]) -> anyhow::Result<()> {
    match command {
        [] => {}
        ["keygen"] => println!("{}", config.keygen(None)?),
        ["keygen", "-"] => {
            let mut stdout = std::io::stdout().lock();
            anyhow::ensure!(
                !stdout.is_terminal(),
                "keygen -: {}",
                message(
                    "pipe the key into systemd-creds or a file, not a terminal",
                    "передайте ключ в systemd-creds или файл, а не в терминал"
                )
            );
            eprintln!("{}", config.keygen(Some(&mut stdout))?);
        }
        ["id"] => print!("{}", id(config)?),
        ["probe-key"] => println!("{}", config.node_key()?.public().to_peer_id()),
        ["status"] => {
            let running = running(config).await;
            print!("{}", status(config, running));
            if !running {
                // The LSB status code for "not running".
                std::process::exit(3);
            }
        }
        ["deny", rest @ ..] => deny(config, rest)?,
        _ => {
            let (usage, client_usage) = usage();
            anyhow::bail!("{usage}\n{client_usage}")
        }
    }
    Ok(())
}

/// The first of `./dyappd.toml`, `~/dyappd.toml` and `/etc/dyappd.toml` that exists.
fn default_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(|home| Path::new(&home).join("dyappd.toml"));
    let etc = Some("/etc/dyappd.toml".into());
    [Some(PathBuf::from("dyappd.toml")), home, etc]
        .into_iter()
        .flatten()
        .find(|file| file.is_file())
}

/// `dyappd deny ...`, with `deny` stripped from `args`.
fn deny_entry(config: &NodeConfig, entry: &str) -> anyhow::Result<String> {
    normalize(entry, &config.limits).ok_or_else(|| {
        anyhow::anyhow!(
            "{entry}: {}",
            message(
                "not a peer ID, a SHA-256 or an IP group of the node's prefix",
                "не peer ID, SHA-256 или группа IP с настроенным префиксом"
            )
        )
    })
}

fn deny_add(
    config: &NodeConfig,
    deny: &DenyStore,
    entry: &str,
    note: &[&str],
) -> anyhow::Result<()> {
    let entry = deny_entry(config, entry)?;
    let added = deny.add(&entry, &note.join(" "))?;
    println!(
        "{entry} {}",
        if added {
            message("added", "добавлено")
        } else {
            message("already listed", "уже в списке")
        }
    );
    Ok(())
}

fn deny_remove(config: &NodeConfig, deny: &DenyStore, entry: &str) -> anyhow::Result<()> {
    let entry = deny_entry(config, entry)?;
    let removed = deny.remove(&entry)?;
    println!(
        "{entry} {}",
        if removed {
            message("removed", "удалено")
        } else {
            message("not listed", "нет в списке")
        }
    );
    Ok(())
}

fn deny_list(deny: &DenyStore) -> anyhow::Result<()> {
    for (entry, note, added) in deny.list()? {
        let added = jiff::Timestamp::new(added, 0).unwrap_or_default();
        println!("{entry}\t{}\t{note}", added.strftime("%Y-%m-%d %H:%M"));
    }
    Ok(())
}

fn deny_received(deny: &DenyStore) -> anyhow::Result<()> {
    for (peer, time, record) in deny.received()? {
        let time = jiff::Timestamp::new(time, 0).unwrap_or_default();
        let list = proto::DenyList::decode(record.payload.as_slice())?;
        println!("{peer}\t{}", time.strftime("%Y-%m-%d %H:%M"));
        for entry in list.entries {
            println!("\t{entry}");
        }
    }
    Ok(())
}

/// `dyappd deny ...`, with `deny` stripped from `args`.
fn deny(config: &NodeConfig, args: &[&str]) -> anyhow::Result<()> {
    let deny = DenyStore::open(&config.storage.deny_path())?;
    match args {
        ["add", entry, note @ ..] => deny_add(config, &deny, entry, note),
        ["remove", entry] => deny_remove(config, &deny, entry),
        ["list"] => deny_list(&deny),
        ["received"] => deny_received(&deny),
        _ => anyhow::bail!(usage().0),
    }
}

/// The peer ID, then one `seeds` entry per `external` address for other nodes to pin this one.
fn id(config: &NodeConfig) -> anyhow::Result<String> {
    let path = config.storage.dir.join("node.id");
    let peer_id = config.node_id().map_err(|e| {
        anyhow::anyhow!(
            "{}: {e}; {}",
            path.display(),
            message(
                "run `dyappd keygen` once",
                "один раз запустите `dyappd keygen`"
            )
        )
    })?;
    let peer_id = peer_id.trim();
    let seeds = config.external.iter().map(|a| format!("{a}/{peer_id}\n"));
    Ok(format!("{peer_id}\n{}", seeds.collect::<String>()))
}

/// Whether the node answers `info` on a listen address; a wildcard host is dialed on loopback.
async fn running(config: &NodeConfig) -> bool {
    for address in &config.listen {
        let local = address
            .replacen("0.0.0.0:", "127.0.0.1:", 1)
            .replacen("[::]:", "[::1]:", 1);
        let reply = client::run(&config.network.name, &["info", &local]).await;
        if matches!(reply, Ok(Some(_))) {
            return true;
        }
    }
    false
}

/// `GET /metrics` from the node's loopback listener; `None` when metrics are off or it does not
/// answer.
fn scrape(config: &NodeConfig) -> Option<String> {
    use std::io::{Read, Write};
    config.metrics.addresses().iter().find_map(|address| {
        let mut stream =
            std::net::TcpStream::connect_timeout(address, Duration::from_secs(2)).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        stream.write_all(b"GET /metrics HTTP/1.0\r\n\r\n").ok()?;
        let mut reply = String::new();
        stream.read_to_string(&mut reply).ok()?;
        let (head, body) = reply.split_once("\r\n\r\n")?;
        head.starts_with("HTTP/1.1 200").then(|| body.to_owned())
    })
}

/// The sum of the `dyapp_<family>` samples whose labels contain `label`.
fn total(metrics: &str, family: &str, label: &str) -> u64 {
    let prefix = format!("dyapp_{family}");
    metrics
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix(&prefix)?;
            if !rest.starts_with(['{', ' ']) || !rest.contains(label) {
                return None;
            }
            rest.rsplit(' ').next()?.parse::<u64>().ok()
        })
        .sum()
}

/// One `name value` line each: health, then store counts (a store not created yet shows
/// `none`), then traffic since the node started (`unavailable` when stopped or metrics are off).
fn count(path: &Path, sql: &str) -> String {
    if !path.exists() {
        return "none".to_string();
    }
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|db| db.query_row(sql, [], |row| row.get::<_, i64>(0)))
        .map_or_else(|e| format!("error: {e}"), |n| n.to_string())
}

fn storage_counts(config: &NodeConfig) -> [(&'static str, String); 8] {
    let storage = &config.storage;
    let media = storage.media_path();
    [
        (
            "profiles",
            count(&storage.profiles_path(), "SELECT count(*) FROM `profiles`"),
        ),
        (
            "envelopes",
            count(&storage.messages_path(), "SELECT count(*) FROM `envelopes`"),
        ),
        (
            "envelope_bytes",
            count(
                &storage.messages_path(),
                "SELECT coalesce(sum(`size`), 0) FROM `envelopes`",
            ),
        ),
        (
            "media_blobs",
            count(
                &media,
                "SELECT count(DISTINCT `hash`) FROM `blobs` WHERE `size` IS NOT NULL",
            ),
        ),
        (
            "media_bytes",
            count(
                &media,
                "SELECT coalesce(sum(`size`), 0) FROM (SELECT DISTINCT `hash`, `size` FROM `blobs`)",
            ),
        ),
        (
            "purged_blobs",
            count(&media, "SELECT `count` FROM `purges` WHERE `id` = 1"),
        ),
        (
            "deny_entries",
            count(&storage.deny_path(), "SELECT count(*) FROM `deny`"),
        ),
        (
            "cached_peers",
            node::cached_peers(&storage.peers_path()).len().to_string(),
        ),
    ]
}

/// One `name value` line each: health, then store counts (a store not created yet shows
/// `none`), then traffic since the node started (`unavailable` when stopped or metrics are off).
fn status(config: &NodeConfig, running: bool) -> String {
    let peer_id = config.node_id().unwrap_or_default();
    let metrics = running.then(|| scrape(config)).flatten();
    let traffic = |value: &dyn Fn(&str) -> u64| {
        metrics
            .as_deref()
            .map_or_else(|| "unavailable".to_string(), |m| value(m).to_string())
    };
    let now = Clock::system().unix_secs();
    let mut rows = vec![
        (
            "node",
            if running { "running" } else { "stopped" }.to_string(),
        ),
        ("peer_id", peer_id.trim().to_string()),
    ];
    rows.extend(storage_counts(config));
    rows.extend([
        (
            "uptime_seconds",
            traffic(&|m| now.saturating_sub(total(m, "process_start_time_seconds", ""))),
        ),
        ("requests", traffic(&|m| total(m, "requests_total", ""))),
        (
            "wire_in_bytes",
            traffic(&|m| total(m, "wire_bytes_total", "direction=\"in\"")),
        ),
        (
            "wire_out_bytes",
            traffic(&|m| total(m, "wire_bytes_total", "direction=\"out\"")),
        ),
    ]);
    rows.iter()
        .flat_map(|(name, value)| [*name, " ", value.as_str(), "\n"])
        .collect()
}
