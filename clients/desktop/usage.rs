//! Settings → Storage: where the app keeps its data and how much it takes (v0.0.1-client-screens.md,
//! Desktop storage and status). A missing number stays `None`, never zero.

use std::fs;
use std::path::Path;

use serde::Serialize;

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct Usage {
    pub identity_dir: Option<String>,
    pub messages_path: String,
    pub messages: Option<u64>,
    /// The installation node store and configured limit; `None` if its path is unavailable.
    pub node_path: Option<String>,
    pub node: Option<u64>,
    pub node_limit: Option<u64>,
    pub free: Option<u64>,
}

/// The status bar; `traffic` is `[in, out]` and `None` until a client swarm starts.
#[derive(Serialize)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
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
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn messages(dir: &Path) -> Option<u64> {
    let siblings = ["client.db-wal", "client.db-shm"].map(|name| bytes(&dir.join(name)));
    bytes(&dir.join("client.db")).map(|used| used + siblings.into_iter().flatten().sum::<u64>())
}

/// The usage of slot folder `dir`; `config` is the app config directory that holds the slots.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn usage(config: Option<&Path>, dir: &Path) -> Usage {
    let (node_path, node, node_limit) = {
        let store = config.map(|root| root.join("node"));
        let limit = config
            .and_then(|root| crate::node::load(root).ok())
            .map(|settings| settings.disk_mb << 20);
        (
            store.as_ref().map(|path| path.display().to_string()),
            store.as_deref().and_then(bytes),
            limit,
        )
    };
    let free = dir
        .to_str()
        .and_then(|path| dyapp_bootstrap::storage::free(path).ok());
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
        let root = std::env::temp_dir().join(format!("dyapp-desktop-usage-кириллица-{nanos}"));
        let dir = root.join("profiles/first");
        fs::create_dir_all(&dir).unwrap();
        let empty = usage(None, &dir);
        assert_eq!(
            (empty.identity_dir, empty.messages, empty.node),
            (None, None, None)
        );
        assert_eq!(
            empty.messages_path,
            dir.join("client.db").display().to_string()
        );

        fs::write(dir.join("client.db"), [0; 100]).unwrap();
        fs::write(dir.join("client.db-wal"), [0; 20]).unwrap();
        fs::create_dir_all(root.join("node/media")).unwrap();
        fs::write(root.join("node/profiles.db"), [0; 7]).unwrap();
        fs::write(root.join("node/media/chunk"), [0; 5]).unwrap();
        assert_eq!(within(&dir.join("node/vanished")), Some(0));
        assert_eq!(bytes(&dir.join("node/vanished")), None);
        let full = usage(Some(&root), &dir);
        assert_eq!(full.identity_dir, Some(root.display().to_string()));
        assert_eq!(full.messages, Some(120));
        assert!(full.free.is_some_and(|free| free > 0));
        {
            assert_eq!(full.node, Some(12));
            assert_eq!(full.node_limit, Some(1024 << 20));
        }
        let other = root.join("profiles/second");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("client.db"), [0; 5]).unwrap();
        let switched = usage(Some(&root), &other);
        assert_eq!(switched.messages, Some(5));
        assert_eq!(switched.node_path, full.node_path);
        assert_eq!(switched.node, full.node);
        assert_eq!(switched.node_limit, full.node_limit);
        fs::remove_dir_all(&root).unwrap();
    }
}
