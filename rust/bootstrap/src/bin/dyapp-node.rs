//! `dyapp-node [--config <file>]`: checks the config, opens the stores and runs the libp2p node.
//! Any config problem stops the node before it opens a store or a socket.

use dyapp_bootstrap::{BootstrapStore, NodeConfig};
use dyapp_p2p_net::{build_swarm, Mode};
use libp2p::futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let mut args = std::env::args().skip(1);
    let file = match (args.next().as_deref(), args.next()) {
        (None, _) => None,
        (Some("--config"), Some(path)) => Some(PathBuf::from(path)),
        _ => anyhow::bail!("usage: dyapp-node [--config <file>]"),
    };
    let (config, ignored) = NodeConfig::load(file.as_deref(), std::env::vars())?;
    for key in ignored {
        tracing::warn!("unknown config key ignored: {key}");
    }
    config.validate()?;
    let keypair = config.node_key()?;
    let store = BootstrapStore::open(
        &config.storage.profiles_path(),
        &config.storage.messages_path(),
    )?;

    let mut swarm = build_swarm(keypair, Mode::Auto)?;
    for address in &config.listen {
        swarm.listen_on(address.parse()?)?;
    }
    for address in &config.external {
        swarm.add_external_address(address.parse()?);
    }
    tracing::info!(peer_id = %swarm.local_peer_id(), roles = ?config.roles, "node started");

    let mut cleanup = tokio::time::interval(Duration::from_secs(3600));
    loop {
        tokio::select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => tracing::info!(%address, "listening"),
                SwarmEvent::ExternalAddrConfirmed { address } => {
                    tracing::info!(%address, "external address confirmed")
                }
                _ => {}
            },
            _ = cleanup.tick() => {
                let now = chrono::Utc::now().timestamp();
                match store.cleanup_expired(now) {
                    Ok(removed) => tracing::info!(removed, "expired messages removed"),
                    Err(error) => tracing::error!(%error, "message cleanup failed"),
                }
            }
        }
    }
}
