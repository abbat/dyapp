use dyapp_p2p_net::{id_has_pow, id_pow_bits};
use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
use std::time::{Duration, Instant};

const HEADER: &str = "dyapp-node-cache 1";
const MAX_BYTES: u64 = 64 * 1024;
const MAX_ENTRIES: usize = 128;
const MAX_GROUP_ENTRIES: usize = 8;
const MAX_PEER_ADDRESSES: usize = 2;
const WRITE_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Candidate {
    pub(crate) peer: PeerId,
    pub(crate) address: Multiaddr,
}

#[derive(Debug)]
struct Inner {
    path: PathBuf,
    network: String,
    entries: Vec<Candidate>,
    dirty: bool,
    last_write: Option<Instant>,
}

/// Untrusted, installation-scoped hints for seed fallback; health remains in memory.
#[derive(Clone, Debug)]
pub(crate) struct Cache {
    inner: Arc<Mutex<Inner>>,
}

type Entries = HashMap<PathBuf, Weak<Mutex<Inner>>>;
static CACHES: OnceLock<Mutex<Entries>> = OnceLock::new();

impl Cache {
    pub(crate) fn at(root: &Path, network: &str) -> Self {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let path = root.join(format!("node-cache-{}", network_key(network)));
        let mut caches = CACHES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        caches.retain(|_, cache| cache.strong_count() != 0);
        if let Some(cache) = caches.get(&path).and_then(Weak::upgrade) {
            return Self { inner: cache };
        }
        let inner = Arc::new(Mutex::new(Inner {
            entries: read(&path, network).unwrap_or_default(),
            path: path.clone(),
            network: network.to_owned(),
            dirty: false,
            last_write: None,
        }));
        caches.insert(path, Arc::downgrade(&inner));
        Self { inner }
    }

    pub(crate) fn record(&self, peer: PeerId, address: Multiaddr) {
        self.record_at(peer, address, Instant::now());
    }

    fn record_at(&self, peer: PeerId, mut address: Multiaddr, now: Instant) {
        if !id_has_pow(&peer, id_pow_bits()) {
            return;
        }
        if !matches!(address.iter().last(), Some(Protocol::P2p(value)) if value == peer) {
            address.push(Protocol::P2p(peer));
        }
        let candidate = Candidate { peer, address };
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if !inner.entries.contains(&candidate) {
            let group = dyapp_p2p_net::ip_group(&candidate.address, false).unwrap_or_default();
            evict(
                &mut inner.entries,
                |entry| dyapp_p2p_net::ip_group(&entry.address, false).unwrap_or_default() == group,
                MAX_GROUP_ENTRIES,
            );
            evict(
                &mut inner.entries,
                |entry| entry.peer == peer,
                MAX_PEER_ADDRESSES,
            );
            evict(&mut inner.entries, |_| true, MAX_ENTRIES);
            inner.entries.push(candidate);
            inner.dirty = true;
        }
        if !inner.dirty
            || inner
                .last_write
                .is_some_and(|written| now.saturating_duration_since(written) < WRITE_INTERVAL)
        {
            return;
        }
        inner.last_write = Some(now);
        if write(&inner).is_ok() {
            inner.dirty = false;
        }
    }

    pub(crate) fn candidates(&self) -> Vec<Candidate> {
        let mut entries = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entries
            .clone();
        shuffle(&mut entries, getrandom::u64().unwrap_or(0xd17));
        entries
    }
}

fn evict(entries: &mut Vec<Candidate>, matches: impl Fn(&Candidate) -> bool, limit: usize) {
    while entries.iter().filter(|entry| matches(entry)).count() >= limit {
        let Some(index) = entries.iter().position(&matches) else {
            break;
        };
        entries.remove(index);
    }
}

fn network_key(network: &str) -> String {
    if network.is_empty() {
        return "public".into();
    }
    let mut key = String::with_capacity(network.len().saturating_mul(2));
    for byte in network.as_bytes() {
        let _ = std::fmt::write(&mut key, format_args!("{byte:02x}"));
    }
    key
}

fn pinned(peer: PeerId, address: Multiaddr) -> Option<Candidate> {
    if !id_has_pow(&peer, id_pow_bits())
        || !matches!(address.iter().last(), Some(Protocol::P2p(value)) if value == peer)
    {
        return None;
    }
    Some(Candidate { peer, address })
}

fn read(path: &Path, network: &str) -> io::Result<Vec<Candidate>> {
    let file = match fs::File::open(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        result => result?,
    };
    let mut text = String::new();
    file.take(MAX_BYTES + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "node cache too large",
        ));
    }
    let mut lines = text
        .strip_suffix('\n')
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "node cache truncated"))?
        .split('\n');
    if lines.next() != Some(HEADER) || lines.next() != Some(format!("name {network}").as_str()) {
        return Err(io::Error::new(ErrorKind::InvalidData, "node cache scope"));
    }
    let mut entries = Vec::new();
    for line in lines {
        let Some(rest) = line.strip_prefix("node ") else {
            return Err(io::Error::new(ErrorKind::InvalidData, "node cache entry"));
        };
        let (peer, address) = rest
            .split_once(' ')
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "node cache fields"))?;
        let candidate = pinned(
            PeerId::from_str(peer).map_err(|_| io::Error::new(ErrorKind::InvalidData, "peer"))?,
            Multiaddr::from_str(address)
                .map_err(|_| io::Error::new(ErrorKind::InvalidData, "address"))?,
        )
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "node cache identity"))?;
        if !entries.contains(&candidate) {
            entries.push(candidate);
        }
        if entries.len() > MAX_ENTRIES {
            return Err(io::Error::new(ErrorKind::InvalidData, "node cache entries"));
        }
    }
    Ok(entries)
}

fn encode(inner: &Inner) -> String {
    let mut text = format!("{HEADER}\nname {}\n", inner.network);
    for entry in &inner.entries {
        let _ = std::fmt::write(
            &mut text,
            format_args!("node {} {}\n", entry.peer, entry.address),
        );
    }
    text
}

fn write(inner: &Inner) -> io::Result<()> {
    let temp = inner.path.with_extension("tmp");
    let _ = fs::remove_file(&temp);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    file.write_all(encode(inner).as_bytes())?;
    file.sync_all()?;
    fs::rename(&temp, &inner.path)?;
    #[cfg(unix)]
    if let Some(parent) = inner.path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn shuffle(entries: &mut [Candidate], mut seed: u64) {
    for index in (1..entries.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let width = u64::try_from(index.saturating_add(1)).unwrap_or(u64::MAX);
        let selected = usize::try_from(seed.checked_rem(width).unwrap_or(0)).unwrap_or(0);
        entries.swap(index, selected);
    }
}

#[cfg(test)]
#[path = "node_cache/tests.rs"]
mod tests;
