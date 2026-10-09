//! `dyappd [--config <file>]`: checks the config, opens the stores and runs the libp2p node.
//! Any config problem stops the node before it opens a store or a socket.
//!
//! `dyappd keygen [--config <file>]`: makes the node key once (about a minute on 2 vCPU) and
//! prints the peer ID; the node does not start without it.

use dyapp_bootstrap::config::Role;
use dyapp_bootstrap::media::MediaStore;
use dyapp_bootstrap::service::Service;
use dyapp_bootstrap::{node, BootstrapStore, NodeConfig};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let keygen = args.first().is_some_and(|a| a == "keygen");
    if keygen {
        args.remove(0);
    }
    let file = match args.as_slice() {
        [] => None,
        [flag, path] if flag == "--config" => Some(PathBuf::from(path)),
        _ => anyhow::bail!("usage: dyappd [keygen] [--config <file>]"),
    };
    let (config, ignored) = NodeConfig::load(file.as_deref(), std::env::vars())?;
    for key in ignored {
        tracing::warn!("unknown config key ignored: {key}");
    }
    config.validate()?;
    dyapp_p2p_net::set_id_pow_bits(config.network.id_pow_bits);
    if keygen {
        println!("{}", config.keygen()?);
        return Ok(());
    }
    let keypair = config.node_key()?;
    let store = BootstrapStore::open(
        &config.storage.profiles_path(),
        &config.storage.messages_path(),
    )?;

    let mut swarm = node::swarm(keypair, &config.limits, &config.roles)?;
    for address in &config.listen {
        swarm.listen_on(address.parse()?)?;
    }
    for address in &config.external {
        swarm.add_external_address(address.parse()?);
    }
    // Seeds are dialled by the first maintenance run of `node::run`.
    let cached = node::cached_peers(&config.storage.peers_path());
    dyapp_p2p_net::join(&mut swarm, &[], &cached);
    tracing::info!(peer_id = %swarm.local_peer_id(), roles = ?config.roles, "node started");
    let media = config.roles.contains(&Role::Media);
    let media = media
        .then(|| MediaStore::open(&config.storage.media_path()))
        .transpose()?;
    let mut service = Service::new(store, config);
    service.media = media;
    node::run(swarm, service).await;
    Ok(())
}
