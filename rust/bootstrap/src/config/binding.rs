//! Binds `storage.dir` to one network through the `network` marker file: a node moved to another
//! network would serve the old network's data in it.

use super::*;

/// The marker holds a network name: at most 64 bytes.
const MAX_MARKER: u64 = 64;

impl NodeConfig {
    /// Checks that `storage.dir` belongs to `network.name`, writing the `network` marker when it
    /// is missing: the configured name for a fresh directory, the public network (empty) for one
    /// with data from before network names. An existing marker is never replaced.
    pub fn bind_network(&self) -> anyhow::Result<()> {
        let path = self.storage.dir.join("network");
        let stored = match read_marker(&path)? {
            Some(stored) => stored,
            None => {
                let stored = if self.has_data()? {
                    ""
                } else {
                    self.network.name.as_str()
                };
                match OpenOptions::new().write(true).create_new(true).open(&path) {
                    Ok(mut file) => {
                        file.write_all(stored.as_bytes())
                            .and_then(|()| file.sync_all())
                            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
                        stored.to_owned()
                    }
                    // Another start or keygen bound it first: its marker wins.
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => read_marker(&path)?
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "{}: {}",
                                path.display(),
                                message("vanished while binding", "исчез во время привязки сети")
                            )
                        })?,
                    Err(e) => anyhow::bail!("{}: {e}", path.display()),
                }
            }
        };
        if stored != self.network.name {
            let owner = if stored.is_empty() {
                message("the public network", "публичной сети").to_owned()
            } else {
                format!("{} {stored:?}", message("network", "сети"))
            };
            if russian_locale() {
                anyhow::bail!(
                    "хранилище принадлежит {owner}, а network.name равно {:?}; укажите новый storage.dir",
                    self.network.name
                );
            }
            anyhow::bail!(
                "storage belongs to {owner}, network.name is {:?}; use a new storage.dir",
                self.network.name
            );
        }
        Ok(())
    }

    /// Whether `storage.dir` holds node data: a store, its SQLite sidecars, the media blobs in
    /// `data/` or the old `media/` layout. The deny list, peer caches and the key are not data.
    pub(super) fn has_data(&self) -> anyhow::Result<bool> {
        let dir = &self.storage.dir;
        let sidecars = self.storage.stores().into_iter().flat_map(|store| {
            ["", "-wal", "-shm", "-journal"].map(|suffix| {
                let mut name = store.clone().into_os_string();
                name.push(suffix);
                PathBuf::from(name)
            })
        });
        for path in sidecars.chain([dir.join("data"), dir.join("media")]) {
            match fs::symlink_metadata(&path) {
                Ok(_) => return Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => anyhow::bail!("{}: {e}", path.display()),
            }
        }
        Ok(false)
    }
}

/// The network name in the marker, `None` when there is none. A marker that is too large, not
/// UTF-8 or not a valid name is refused.
fn read_marker(path: &Path) -> anyhow::Result<Option<String>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => anyhow::bail!("{}: {e}", path.display()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_MARKER + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_MARKER,
        "{}: {}",
        path.display(),
        message("longer than a network name", "имя сети слишком длинное")
    );
    let name = String::from_utf8(bytes).map_err(|_| {
        anyhow::anyhow!(
            "{}: {}",
            path.display(),
            message("not a network name", "неверное имя сети")
        )
    })?;
    dyapp_p2p_net::Protocols::new(&name).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    Ok(Some(name))
}
