use super::*;

impl NodeConfig {
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
        self.network.dial_filter()?;
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
        // Store-only nodes also advertise this policy. Other search settings remain
        // irrelevant when search is disabled.
        if self.roles.contains(&Role::Store) && self.search.min_profile_pow_bits > 20 {
            anyhow::bail!("search.min_profile_pow_bits must be at most 20");
        }
        if self.roles.contains(&Role::Search) {
            // As with turn: a client-mode node serves no protocol.
            if !self.roles.contains(&Role::Store) {
                anyhow::bail!("role search needs role store");
            }
            self.search.validate()?;
            // A public node answers for every topic; a subset would look complete to clients.
            if self.network.name.is_empty() && !self.search.topics.is_empty() {
                anyhow::bail!("search.topics: the public network keeps all topics; leave it empty");
            }
        }
        let l = &self.limits;
        if l.message_ttl_hours < 1
            || l.attachment_retention_hours < 1
            || l.media_cache_minutes < 1
            || l.profile_ttl_days < 1
        {
            anyhow::bail!(
                "limits.message_ttl_hours, attachment_retention_hours, media_cache_minutes and \
                 profile_ttl_days must be at least 1"
            );
        }
        // A 1 KiB signal payload with its key and signature encodes to more than 1 KiB.
        if l.signal_max_kb < 2 {
            anyhow::bail!("limits.signal_max_kb must be at least 2");
        }
        // One largest envelope must fit the pool.
        if l.mailbox_pool_kb < 101 || l.tokenless_share_pct > 100 {
            anyhow::bail!(
                "limits.mailbox_pool_kb must be at least 101 and tokenless_share_pct at most 100"
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
        // The coarse bucket refills every 1 / (10 x rate) s, and governor counts nanoseconds.
        if l.ip_group_requests_per_second > 100_000_000 {
            anyhow::bail!("limits.ip_group_requests_per_second must be at most 100000000 for the 10x coarse budget");
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
}

fn check_writable(dir: &Path) -> anyhow::Result<()> {
    let probe = dir.join(".write-probe");
    fs::create_dir_all(dir)
        .and_then(|()| fs::write(&probe, b""))
        .and_then(|()| fs::remove_file(&probe))
        .map_err(|e| anyhow::anyhow!("{} is not writable by this user: {e}", dir.display()))
}
