//! The operator's deny list in `<storage.dir>/deny.db`, edited by `dyappd deny` while the node
//! runs. An entry is a libp2p peer ID, an IP group as the node computes it (`203.0.113.0/24`) or
//! the lowercase hex SHA-256 of an identity or device key or of a media blob.

use crate::config::Limits;
use crate::error::Result;
use crate::node::ip_group;
use crate::storage::{lock, open, storage_error};
use libp2p::PeerId;
use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

pub struct DenyStore {
    db: Mutex<Connection>,
}

/// The entry in its stored form, or `None` when it is no peer ID, SHA-256 or IP group with the
/// prefix of `limits` (an address in the group is masked: `203.0.113.77/24` is `203.0.113.0/24`).
pub fn normalize(entry: &str, limits: &Limits) -> Option<String> {
    let entry = entry.trim();
    if entry.parse::<PeerId>().is_ok() {
        return Some(entry.to_string());
    }
    let hash = entry.to_ascii_lowercase();
    if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Some(hash);
    }
    let (ip, prefix) = entry.split_once('/')?;
    let ip: std::net::IpAddr = ip.parse().ok()?;
    let want = if ip.is_ipv4() {
        limits.ipv4_prefix
    } else {
        limits.ipv6_prefix
    };
    (prefix.parse() == Ok(want)).then(|| ip_group(&ip.into(), limits))
}

impl DenyStore {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            db: open(
                path,
                "CREATE TABLE IF NOT EXISTS deny (
                     entry TEXT PRIMARY KEY,
                     note TEXT NOT NULL,
                     added INTEGER NOT NULL
                 );",
            )?,
        })
    }

    /// Adds an entry; false when it was listed already.
    pub fn add(&self, entry: &str, note: &str) -> Result<bool> {
        let added = lock(&self.db)?
            .execute(
                "INSERT OR IGNORE INTO deny (entry, note, added) VALUES (?, ?, ?)",
                params![entry, note, chrono::Utc::now().timestamp()],
            )
            .map_err(storage_error)?;
        Ok(added > 0)
    }

    /// Removes an entry; false when it was not listed.
    pub fn remove(&self, entry: &str) -> Result<bool> {
        let removed = lock(&self.db)?
            .execute("DELETE FROM deny WHERE entry = ?", [entry])
            .map_err(storage_error)?;
        Ok(removed > 0)
    }

    /// Every entry with its note and the Unix time it was added, oldest first.
    pub fn list(&self) -> Result<Vec<(String, String, i64)>> {
        let db = lock(&self.db)?;
        let mut statement = db
            .prepare("SELECT entry, note, added FROM deny ORDER BY added, entry")
            .map_err(storage_error)?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(storage_error)?;
        rows.collect::<rusqlite::Result<_>>().map_err(storage_error)
    }

    // ponytail: the node holds every entry in memory; query the indexed table per request if
    // lists grow past a few hundred thousand entries.
    pub fn entries(&self) -> Result<HashSet<String>> {
        Ok(self.list()?.into_iter().map(|(entry, ..)| entry).collect())
    }

    /// A counter that changes when another connection commits, so the node reloads only then.
    pub fn version(&self) -> Result<i64> {
        lock(&self.db)?
            .query_row("PRAGMA data_version", [], |row| row.get(0))
            .map_err(storage_error)
    }

    /// Moves the entries of the old `<storage.dir>/deny` file in, once: the file is renamed to
    /// `deny.imported`. Returns the entries read; ones that do not parse are logged and skipped.
    pub fn import(&self, file: &Path, limits: &Limits) -> Result<usize> {
        let text = match std::fs::read_to_string(file) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            text => text.map_err(storage_error)?,
        };
        let mut count = 0;
        for line in text.lines() {
            let (entry, note) = line.split_once('#').unwrap_or((line, ""));
            if entry.trim().is_empty() {
                continue;
            }
            match normalize(entry, limits) {
                Some(entry) => {
                    self.add(&entry, note.trim())?;
                    count += 1;
                }
                None => tracing::warn!(entry = entry.trim(), "deny file entry skipped"),
            }
        }
        std::fs::rename(file, file.with_file_name("deny.imported")).map_err(storage_error)?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_store_adds_removes_and_imports_the_old_file() {
        let dir = std::env::temp_dir().join(format!("dyapp-deny-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = DenyStore::open(&dir.join("deny.db")).unwrap();
        let peer = PeerId::random().to_string();
        let hash = "AB".repeat(32);
        std::fs::write(
            dir.join("deny"),
            format!("# operator list\n{peer}  # spam\n\n203.0.113.0/24\n{hash}\nbogus\n"),
        )
        .unwrap();
        let limits = Limits::default();
        assert_eq!(store.import(&dir.join("deny"), &limits).unwrap(), 3);
        assert!(!dir.join("deny").exists() && dir.join("deny.imported").exists());
        let entries = store.entries().unwrap();
        assert!(entries.contains(&peer) && entries.contains("203.0.113.0/24"));
        assert!(entries.contains(&hash.to_lowercase()));
        assert!(store
            .list()
            .unwrap()
            .iter()
            .any(|(e, n, _)| *e == peer && n == "spam"));
        assert!(!store.add("203.0.113.0/24", "").unwrap());
        assert!(store.remove("203.0.113.0/24").unwrap());
        assert!(!store.remove("203.0.113.0/24").unwrap());
        let n = |entry| normalize(entry, &limits);
        assert_eq!(n("2001:db8:1:5::/48").as_deref(), Some("2001:db8:1::/48"));
        assert_eq!(n("203.0.113.77/24").as_deref(), Some("203.0.113.0/24"));
        assert_eq!(n("203.0.113.0/16"), None);
        assert_eq!(n("bogus"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
