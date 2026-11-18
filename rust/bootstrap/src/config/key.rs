//! The node key: made once with its proof of work, then loaded with the data directory binding.

use super::{
    fs, message, read_string, File, Keypair, NodeConfig, OpenOptions, Path, PeerId, Read, Write,
    Zeroizing, MAX_NODE_ID_BYTES,
};
#[cfg(unix)]
use super::{MetadataExt, OpenOptionsExt};

pub(super) const MAX_KEY_BYTES: u64 = 4096;

fn sync_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

pub(super) fn persist_new(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    persist_new_with(
        path,
        |file| {
            file.write_all(bytes)?;
            file.sync_all()
        },
        sync_directory,
    )
}

pub(super) fn persist_new_with(
    path: &Path,
    write: impl FnOnce(&mut File) -> std::io::Result<()>,
    mut sync: impl FnMut(&Path) -> std::io::Result<()>,
) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{}: missing parent directory", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("{}: missing file name", path.display()))?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&temp)
        .map_err(|e| anyhow::anyhow!("{}: {e}", temp.display()))?;
    write(&mut file).map_err(|e| anyhow::anyhow!("{}: {e}", temp.display()))?;
    drop(file);
    // A hard link publishes the complete inode without replacing a concurrently created file.
    fs::hard_link(&temp, path).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    let directory = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    sync(directory).map_err(|e| anyhow::anyhow!("{}: {e}", directory.display()))?;
    fs::remove_file(&temp).map_err(|e| anyhow::anyhow!("{}: {e}", temp.display()))?;
    sync(directory).map_err(|e| anyhow::anyhow!("{}: {e}", directory.display()))?;
    Ok(())
}

fn check_permissions(metadata: &fs::Metadata, check_mode: bool, path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    if check_mode && metadata.mode() & 0o077 != 0 {
        anyhow::bail!(
            "{} {}",
            path.display(),
            message(
                "has group/other permissions: use chmod 600 before starting",
                "доступен группе или другим пользователям: перед запуском выполните chmod 600"
            )
        );
    }
    #[cfg(not(unix))]
    let _ = (metadata, check_mode, path);
    Ok(())
}

impl NodeConfig {
    /// Makes the node key with the proof of work of `network.id_pow_bits` (about a minute on
    /// 2 vCPU) and binds the data directory to it and to `network.name`: a new key is a new node.
    /// The key goes to `out` when given (to be sealed by another tool), else to a new file (0600
    /// on Unix,
    /// inherited user-directory access control on Windows). A valid file-backed key with a missing
    /// node ID is recovered; other existing keys, IDs and node data are refused.
    ///
    /// # Errors
    ///
    /// Returns an error if a key, node ID or existing node data is present, random generation,
    /// network binding, key encoding, output writing or node-ID persistence fails.
    pub fn keygen(&self, out: Option<&mut dyn Write>) -> anyhow::Result<PeerId> {
        self.keygen_with(out, persist_new)
    }

    pub(super) fn keygen_with(
        &self,
        out: Option<&mut dyn Write>,
        mut persist: impl FnMut(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> anyhow::Result<PeerId> {
        let key_path = self.storage.key_path();
        let id_path = self.storage.dir.join("node.id");
        if out.is_none() && key_path.exists() && !id_path.exists() {
            return self.recover_key(&key_path, &id_path, &mut persist);
        }
        Self::refuse_existing(&key_path, &id_path)?;
        self.fresh_key(out, &key_path, &id_path, &mut persist)
    }

    fn recover_key(
        &self,
        key_path: &Path,
        id_path: &Path,
        persist: &mut impl FnMut(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> anyhow::Result<PeerId> {
        let keypair = Self::read_key(key_path, true)?;
        let peer_id = self.validate_pow(key_path, &keypair)?;
        self.bind_network()?;
        persist(id_path, peer_id.to_string().as_bytes())?;
        Ok(peer_id)
    }

    fn refuse_existing(key_path: &Path, id_path: &Path) -> anyhow::Result<()> {
        if let Some(file) = [key_path, id_path].into_iter().find(|s| s.exists()) {
            anyhow::bail!(
                "{} {}",
                file.display(),
                message(
                    "exists: a new key is a new node, delete the data to start from scratch",
                    "уже существует: новый ключ создаёт новый узел; для начала с нуля удалите данные"
                )
            );
        }
        Ok(())
    }

    fn fresh_key(
        &self,
        out: Option<&mut dyn Write>,
        key_path: &Path,
        id_path: &Path,
        persist: &mut impl FnMut(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> anyhow::Result<PeerId> {
        if self.has_data()? {
            anyhow::bail!(
                "{} {}",
                self.storage.dir.display(),
                message(
                    "holds node data: a new key is a new node, delete the data to start from scratch",
                    "содержит данные узла: новый ключ создаёт новый узел; для начала с нуля удалите данные"
                )
            );
        }
        self.bind_network()?;
        let keypair = dyapp_p2p_net::generate_pow_keypair(self.network.id_pow_bits)?;
        let encoded = Zeroizing::new(keypair.to_protobuf_encoding()?);
        if let Some(out) = out {
            out.write_all(&encoded)?;
            out.flush()?;
        } else {
            persist(key_path, &encoded)?;
        }
        let peer_id = keypair.public().to_peer_id();
        persist(id_path, peer_id.to_string().as_bytes())?;
        Ok(peer_id)
    }

    /// Loads the node key made by [`Self::keygen`]. A key that does not belong to the stored data
    /// or lacks the proof of work is refused, as is a Unix key file with group/other permission bits.
    ///
    /// # Errors
    ///
    /// Returns an error if key or node-ID files cannot be read, the key file has unsafe permissions
    /// or changes during reading, the key is malformed, or its proof of work or stored node ID differs.
    pub fn node_key(&self) -> anyhow::Result<Keypair> {
        self.load_key(
            std::env::var_os("CREDENTIALS_DIRECTORY")
                .as_deref()
                .map(Path::new),
        )
    }

    fn read_key(key_path: &Path, check_mode: bool) -> anyhow::Result<Keypair> {
        let mut file = match File::open(key_path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => anyhow::bail!(
                "{} {}",
                key_path.display(),
                message(
                    "is missing: run `dyappd keygen` once and keep the key safe",
                    "не найден: один раз выполните `dyappd keygen` и сохраните ключ в надёжном месте"
                )
            ),
            Err(e) => return Err(anyhow::anyhow!("{}: {e}", key_path.display())),
        };
        // Check the opened file, not a path that could be replaced before reading.
        let metadata = file
            .metadata()
            .map_err(|e| anyhow::anyhow!("{}: {e}", key_path.display()))?;
        check_permissions(&metadata, check_mode, key_path)?;
        anyhow::ensure!(
            metadata.len() <= MAX_KEY_BYTES,
            "{} {}",
            key_path.display(),
            message("exceeds 4096 bytes", "превышает 4096 байт")
        );
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
            anyhow::bail!(
                "{} {}",
                key_path.display(),
                message(
                    "grew while reading; retry startup",
                    "изменился во время чтения; повторите запуск"
                )
            );
        }
        Keypair::from_protobuf_encoding(&bytes)
            .map_err(|e| anyhow::anyhow!("{}: {e}", key_path.display()))
    }

    fn validate_pow(&self, key_path: &Path, keypair: &Keypair) -> anyhow::Result<PeerId> {
        let peer_id = keypair.public().to_peer_id();
        if !dyapp_p2p_net::id_has_pow(&peer_id, self.network.id_pow_bits) {
            anyhow::bail!(
                "{} {} {} {}",
                key_path.display(),
                message(
                    "lacks the node-ID proof of work of",
                    "не содержит доказательства работы идентификатора узла сложностью"
                ),
                self.network.id_pow_bits,
                message(
                    "bits: make a new node with `dyappd keygen`",
                    "бит: создайте новый узел через `dyappd keygen`"
                )
            );
        }
        Ok(peer_id)
    }

    fn validate_key(&self, key_path: &Path, keypair: &Keypair) -> anyhow::Result<()> {
        let peer_id = self.validate_pow(key_path, keypair)?;
        let id_path = self.storage.dir.join("node.id");
        let stored: PeerId = read_string(&id_path, MAX_NODE_ID_BYTES)
            .map_err(|e| anyhow::anyhow!("{}: {e}", id_path.display()))?
            .trim()
            .parse()
            .map_err(|e| anyhow::anyhow!("{}: {e}", id_path.display()))?;
        if stored != peer_id {
            anyhow::bail!(
                "{} {} {} (node {stored}): {}",
                key_path.display(),
                message("does not match the data in", "не соответствует данным в"),
                self.storage.dir.display(),
                message(
                    "a new key is a new node, delete the data to start from scratch",
                    "новый ключ создаёт новый узел; для начала с нуля удалите данные"
                )
            );
        }
        Ok(())
    }

    pub(super) fn load_key(&self, credentials: Option<&Path>) -> anyhow::Result<Keypair> {
        let (key_path, check_mode) = self.storage.key_source(credentials);
        let keypair = Self::read_key(&key_path, check_mode)?;
        self.validate_key(&key_path, &keypair)?;
        Ok(keypair)
    }
}
