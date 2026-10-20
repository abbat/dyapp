//! The node key: made once with its proof of work, then loaded with the data directory binding.

use super::*;

impl NodeConfig {
    /// Makes the node key with the proof of work of `network.id_pow_bits` (about a minute on
    /// 2 vCPU) and binds the data directory to it and to `network.name`. Refuses an existing key,
    /// existing data or a directory of another network: a new key is a new node. The key goes to
    /// `out` when given (to be sealed by another tool), else to a new 0600 file.
    pub fn keygen(&self, out: Option<&mut dyn Write>) -> anyhow::Result<PeerId> {
        let key_path = self.storage.key_path();
        let id_path = self.storage.dir.join("node.id");
        if let Some(file) = [&key_path, &id_path].into_iter().find(|s| s.exists()) {
            anyhow::bail!(
                "{} exists: a new key is a new node, delete the data to start from scratch",
                file.display()
            );
        }
        if self.has_data()? {
            anyhow::bail!(
                "{} holds node data: a new key is a new node, delete the data to start from scratch",
                self.storage.dir.display()
            );
        }
        self.bind_network()?;
        let keypair = dyapp_p2p_net::generate_pow_keypair(self.network.id_pow_bits);
        let encoded = Zeroizing::new(keypair.to_protobuf_encoding()?);
        match out {
            Some(out) => {
                out.write_all(&encoded)?;
                out.flush()?;
            }
            None => OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&key_path)
                .map_err(|e| anyhow::anyhow!("{}: {e}", key_path.display()))?
                .write_all(&encoded)?,
        }
        let peer_id = keypair.public().to_peer_id();
        fs::write(&id_path, peer_id.to_string())?;
        Ok(peer_id)
    }

    /// Loads the node key made by [`Self::keygen`]. A key that does not belong to the stored data
    /// or lacks the proof of work is refused, as is a key file with group/other permission bits.
    pub fn node_key(&self) -> anyhow::Result<Keypair> {
        self.load_key(
            std::env::var_os("CREDENTIALS_DIRECTORY")
                .as_deref()
                .map(Path::new),
        )
    }

    pub(super) fn load_key(&self, credentials: Option<&Path>) -> anyhow::Result<Keypair> {
        let (key_path, check_mode) = self.storage.key_source(credentials);
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
            if check_mode && metadata.mode() & 0o077 != 0 {
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
