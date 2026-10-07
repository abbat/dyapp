//! Node protocol client for scripts that have no libp2p, Ed25519 or protobuf library. Each command
//! prints one JSON object; a reply of any status exits 0, a transport failure exits non-zero.
//!
//! - `test-peer sign-profile`: `{"peer_id", "record"}`, a freshly signed profile as hex protobuf;
//! - `test-peer info <multiaddr>`: `{"status", "roles", "max_profile_bytes"}`;
//! - `test-peer publish <multiaddr> <record hex>`: `{"status"}`, plus `"record"` when stale;
//! - `test-peer get <multiaddr> <peer_id hex>`: `{"status", "record"}`.

use anyhow::{anyhow, bail, Context};
use dyapp_identity::Identity;
use dyapp_p2p_net::proto::{self, node_request, profile_request, Status};
use dyapp_p2p_net::{build_swarm, Behaviour, BehaviourEvent, Mode};
use dyapp_profile::Profile;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::request_response::{Event, Message as RrMessage};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use prost::Message;
use serde_json::{json, Value};
use std::future::Future;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = match args.as_slice() {
        ["sign-profile"] => sign_profile(),
        ["info", addr] => timeout(info(addr)).await?,
        ["publish", addr, record] => timeout(publish(addr, record)).await?,
        ["get", addr, peer_id] => timeout(get(addr, peer_id)).await?,
        _ => bail!("usage: test-peer sign-profile | info <addr> | publish <addr> <hex> | get <addr> <peer_id>"),
    };
    println!("{output}");
    Ok(())
}

fn sign_profile() -> Value {
    let identity = Identity::generate();
    let record = Profile {
        version: 1,
        age: 26,
        ..Profile::default()
    }
    .sign(&identity);
    json!({ "peer_id": identity.peer_id(), "record": hex(&record.encode_to_vec()) })
}

async fn timeout(request: impl Future<Output = anyhow::Result<Value>>) -> anyhow::Result<Value> {
    tokio::time::timeout(Duration::from_secs(15), request)
        .await
        .context("no reply in 15 s")?
}

async fn info(addr: &str) -> anyhow::Result<Value> {
    let (mut swarm, peer) = connect(addr).await?;
    let request = proto::NodeRequest {
        request: Some(node_request::Request::Info(proto::InfoRequest {})),
    };
    swarm.behaviour_mut().node.send_request(&peer, request);
    let response = wait(&mut swarm, |event| match event {
        BehaviourEvent::Node(event) => reply(event),
        _ => None,
    })
    .await?;
    let info = response.info.unwrap_or_default();
    let roles: Vec<&str> = info
        .roles
        .iter()
        .map(|role| proto::Role::try_from(*role).map_or("unknown", |role| role.as_str_name()))
        .collect();
    Ok(json!({
        "status": status(response.status),
        "roles": roles,
        "max_profile_bytes": info.max_profile_bytes,
    }))
}

async fn publish(addr: &str, record: &str) -> anyhow::Result<Value> {
    let record = dyapp_identity::SignedRecord::decode(unhex(record)?.as_slice())?;
    profile(addr, profile_request::Request::Publish(record)).await
}

async fn get(addr: &str, peer_id: &str) -> anyhow::Result<Value> {
    let peer_id = unhex(peer_id)?;
    profile(
        addr,
        profile_request::Request::Get(proto::GetProfile { peer_id }),
    )
    .await
}

async fn profile(addr: &str, request: profile_request::Request) -> anyhow::Result<Value> {
    let (mut swarm, peer) = connect(addr).await?;
    let request = proto::ProfileRequest {
        request: Some(request),
    };
    swarm.behaviour_mut().profile.send_request(&peer, request);
    let response = wait(&mut swarm, |event| match event {
        BehaviourEvent::Profile(event) => reply(event),
        _ => None,
    })
    .await?;
    let mut output = json!({ "status": status(response.status) });
    if let Some(record) = response.record {
        output["record"] = hex(&record.encode_to_vec()).into();
    }
    Ok(output)
}

/// Dials `addr` and returns the peer that answered: the address needs no `/p2p/` suffix.
async fn connect(addr: &str) -> anyhow::Result<(Swarm<Behaviour>, PeerId)> {
    let mut swarm = build_swarm(Keypair::generate_ed25519(), Mode::Client)?;
    swarm.dial(addr.parse::<Multiaddr>()?)?;
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => return Ok((swarm, peer_id)),
            SwarmEvent::OutgoingConnectionError { error, .. } => bail!("dial {addr}: {error}"),
            _ => {}
        }
    }
}

async fn wait<R>(
    swarm: &mut Swarm<Behaviour>,
    pick: impl Fn(BehaviourEvent) -> Option<anyhow::Result<R>>,
) -> anyhow::Result<R> {
    loop {
        if let SwarmEvent::Behaviour(event) = swarm.select_next_some().await {
            if let Some(result) = pick(event) {
                return result;
            }
        }
    }
}

fn reply<Req, Resp>(event: Event<Req, Resp>) -> Option<anyhow::Result<Resp>> {
    match event {
        Event::Message {
            message: RrMessage::Response { response, .. },
            ..
        } => Some(Ok(response)),
        Event::OutboundFailure { error, .. } => Some(Err(anyhow!("request failed: {error}"))),
        _ => None,
    }
}

fn status(value: i32) -> &'static str {
    Status::try_from(value).map_or("unknown", |status| status.as_str_name())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> anyhow::Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        bail!("odd-length hex");
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).context("bad hex"))
        .collect()
}
