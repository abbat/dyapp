//! Settings → Storage: where the app keeps its data and how much it takes (v0.0.1-client-screens.md,
//! Desktop storage and status). A missing number stays `None`, never zero.

use std::fs;
use std::path::Path;

use serde::Serialize;

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub identity_dir: Option<String>,
    pub messages_path: String,
    pub messages: Option<u64>,
    /// The node store and its configured limit; `None` where the app has no node (Windows).
    pub node_path: Option<String>,
    pub node: Option<u64>,
    pub node_limit: Option<u64>,
    pub free: Option<u64>,
}

/// The status bar; `traffic` is `[in, out]` and `None` until a client swarm starts.
#[derive(Serialize)]
pub struct Status {
    pub traffic: Option<[u64; 2]>,
    pub messages: Option<u64>,
}

/// Bytes of the file or of every file under the directory `path`; `None` when `path` is missing or
/// any file is unreadable.
fn bytes(path: &Path) -> Option<u64> {
    fs::symlink_metadata(path).ok()?;
    within(path)
}

/// As [`bytes`], but a file that vanishes during the walk takes no space: a running node removes
/// its temporary files while the card is measured.
fn within(path: &Path) -> Option<u64> {
    let gone = |e: &std::io::Error| e.kind() == std::io::ErrorKind::NotFound;
    let meta = match fs::symlink_metadata(path) {
        Err(e) if gone(&e) => return Some(0),
        meta => meta.ok()?,
    };
    if !meta.is_dir() {
        return Some(meta.len());
    }
    match fs::read_dir(path) {
        Err(e) if gone(&e) => Some(0),
        entries => entries.ok()?.map(|entry| within(&entry.ok()?.path())).sum(),
    }
}

/// Bytes of the client store in slot folder `dir` with its `-wal`/`-shm` siblings.
pub fn messages(dir: &Path) -> Option<u64> {
    let siblings = ["client.db-wal", "client.db-shm"].map(|name| bytes(&dir.join(name)));
    bytes(&dir.join("client.db")).map(|used| used + siblings.into_iter().flatten().sum::<u64>())
}

/// The usage of slot folder `dir`; `config` is the app config directory that holds the slots.
pub fn usage(config: Option<&Path>, dir: &Path) -> Usage {
    #[cfg(unix)]
    let (node_path, node, node_limit) = {
        let store = dir.join("node");
        let limit = crate::node::load(dir).ok().map(|settings| settings.disk_mb << 20);
        (Some(store.display().to_string()), bytes(&store), limit)
    };
    #[cfg(not(unix))]
    let (node_path, node, node_limit) = (None, None, None);
    #[cfg(unix)]
    let free = dir.to_str().and_then(|path| dyapp_bootstrap::storage::free(path).ok());
    #[cfg(not(unix))]
    let free = None;
    Usage {
        identity_dir: config.map(|path| path.display().to_string()),
        messages_path: dir.join("client.db").display().to_string(),
        messages: messages(dir),
        node_path,
        node,
        node_limit,
        free,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_sums_the_store_files_and_leaves_missing_ones_unknown() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-desktop-usage-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        let empty = usage(None, &dir);
        assert_eq!((empty.identity_dir, empty.messages, empty.node), (None, None, None));
        assert_eq!(empty.messages_path, dir.join("client.db").display().to_string());

        fs::write(dir.join("client.db"), [0; 100]).unwrap();
        fs::write(dir.join("client.db-wal"), [0; 20]).unwrap();
        fs::create_dir_all(dir.join("node/media")).unwrap();
        fs::write(dir.join("node/profiles.db"), [0; 7]).unwrap();
        fs::write(dir.join("node/media/chunk"), [0; 5]).unwrap();
        assert_eq!(within(&dir.join("node/vanished")), Some(0));
        assert_eq!(bytes(&dir.join("node/vanished")), None);
        let full = usage(Some(Path::new("/config")), &dir);
        assert_eq!(full.identity_dir.as_deref(), Some("/config"));
        assert_eq!(full.messages, Some(120));
        #[cfg(unix)]
        {
            assert_eq!(full.node, Some(12));
            assert_eq!(full.node_limit, Some(1024 << 20));
            assert!(full.free.is_some_and(|free| free > 0));
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
