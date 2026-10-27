//! Opening a node from its config, for `dyappd` and the desktop app (ADR 0030).

use super::*;
use crate::config::transports;
use crate::media::MediaStore;
use crate::metrics::{self, listener};
use crate::search::SearchIndex;
use crate::{BootstrapStore, NodeConfig};

/// A node [`open`] made: listening and joining the network, not yet serving.
pub struct Opened {
    /// The node's peer ID.
    pub peer_id: PeerId,
    /// The request handler [`Opened::run`] serves with.
    pub service: Arc<Service>,
    swarm: Swarm<Behaviour>,
    // Held while the node runs, so a second node cannot open the same stores.
    _lock: fs::File,
}

impl Opened {
    /// Serves requests until the future is dropped; see [`run`].
    pub async fn run(self) {
        let Self {
            swarm,
            service,
            _lock,
            ..
        } = self;
        run(swarm, service).await;
    }
}

/// Locks `config.storage`, reads the node key, opens the stores of the configured roles and the
/// metrics listener, listens on `config.listen` and dials the anchors and cached peers; seeds are
/// dialled by the first maintenance run of [`Opened::run`]. `config` must be validated, with its
/// secrets resolved. The metrics listener, if any, runs for the rest of the process.
pub async fn open(config: NodeConfig) -> anyhow::Result<Opened> {
    let lock = config.storage.lock()?;
    let keypair = config.node_key()?;
    config.bind_network()?;
    let scrapes = match config.metrics.addresses() {
        addresses if addresses.is_empty() => None,
        addresses => Some(listener::bind(&addresses).await?),
    };
    let deny_signer = config
        .network
        .share_deny_list
        .then(|| crate::deny::signer(&keypair))
        .flatten();
    let store = BootstrapStore::open(
        &config.storage.profiles_path(),
        &config.storage.messages_path(),
    )?;
    let wire = Arc::new(dyapp_p2p_net::WireBytes::default());
    let protocols = Arc::new(dyapp_p2p_net::Protocols::new(&config.network.name)?);
    let dial_filter = config.network.dial_filter()?;
    let mut swarm = swarm_with_protocols(
        keypair,
        &config.limits,
        &config.roles,
        wire.clone(),
        protocols.clone(),
        config.network.guard_dials,
        dial_filter.clone(),
        config.relay_hop(),
    )?;
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
    // Anchors are dialled first and the cache fills the routing table.
    for anchor in cached_peers(&config.storage.anchors_path()) {
        if let Err(error) = swarm.dial(anchor.clone()) {
            tracing::warn!(%anchor, %error, "anchor not dialed");
        }
    }
    let cached = cached_peers(&config.storage.peers_path());
    dyapp_p2p_net::join(&mut swarm, &[], &cached);
    let peer_id = *swarm.local_peer_id();
    tracing::info!(%peer_id, roles = ?config.roles, "node started");
    let media = config.roles.contains(&Role::Media);
    let media = media
        .then(|| MediaStore::open(&config.storage.dir))
        .transpose()?;
    let mut service = Service::new(store, config);
    service.wire = wire;
    service.dial_filter = dial_filter;
    service.protocols = protocols;
    service.media = media;
    if service.config.roles.contains(&Role::Search) {
        let search = &service.config;
        let index = SearchIndex::open(&search.storage.search_path(), &search.search)?;
        service.search = Some(Arc::new(index));
    }
    service.deny_signer = deny_signer;
    let service = Arc::new(service);
    if let Some(scrapes) = scrapes {
        metrics::spawn_snapshots(&service);
        tokio::spawn(listener::serve(scrapes, service.clone()));
    }
    Ok(Opened {
        peer_id,
        service,
        swarm,
        _lock: lock,
    })
}
