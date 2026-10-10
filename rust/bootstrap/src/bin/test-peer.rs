//! Node protocol client for scripts that have no libp2p, Ed25519 or protobuf library. Each command
//! prints one JSON object; a reply of any status exits 0, a transport failure exits non-zero.
//!
//! - `test-peer sign-profile [secret version]`: `{"peer_id", "record", "secret"}`, a signed profile;
//! - `test-peer heartbeat <multiaddr> <secret hex>`: `{"status"}`, signed owner presence;
//! - `test-peer info <multiaddr>`: `{"status", "peer_id", "roles", "max_profile_bytes"}`;
//! - `test-peer publish <multiaddr> <record hex>`: `{"status"}`, plus `"record"` when stale;
//! - `test-peer get <multiaddr> <peer_id hex>`: `{"status", "record"}`;
//! - `test-peer device-key`: `{"secret", "mailbox"}`, a fresh device key and its mailbox address;
//! - `test-peer put <multiaddr> <mailbox hex> <ciphertext hex>`: `{"status", "id"}`, an envelope
//!   signed by a fresh sender key;
//! - `test-peer fetch <multiaddr> <secret hex>`: `{"status", "ids", "more"}`;
//! - `test-peer ack <multiaddr> <secret hex> <id hex>...`: `{"status"}`;
//! - `test-peer closest <multiaddr> <key hex>`: `{"peers"}`, the nodes a DHT lookup through the
//!   node finds closest to the key, closest first;
//! - `test-peer replicate <multiaddr> <peer_id hex> <record hex>`: `{"holders"}`, publishes each
//!   of the `REPLICAS` replicas of a profile to the node closest to its replica key;
//! - `test-peer flood <multiaddr> <n>`: the count of each status (`"failed"` for no reply) of `n`
//!   profile gets sent at once on one connection;
//! - `test-peer watch <multiaddr> <secret hex>`: `{"status", "id", "pushed"}`, fetches with
//!   `watch`, puts an envelope to the mailbox from another connection and returns the ids the
//!   node pushed back;
//! - `test-peer put-replicas <multiaddr> <mailbox hex>`: `{"id", "holders"}`, puts one envelope
//!   to the node closest to each replica key of the mailbox;
//! - `test-peer media <multiaddr> <data hex>`: `{"keep", "put", "get"}`, the status of each
//!   step: a fresh owner keeps the blob's hash, puts the blob and gets it back; `"get"` is
//!   `"CHANGED"` if the blob came back different;
//! - `test-peer media-sized <multiaddr> <bytes> <32-byte seed hex>`: the same round trip
//!   with owner details, repeating the seed to generate a large blob without a large CLI argument;
//! - `test-peer turn <multiaddr>`: `{"status", "username", "password", "urls", "expires"}`;
//! - `test-peer relays <multiaddr>`: `{"providers"}`, the TURN relays the DHT knows.
//!
//! `fetch` and `ack` ask for a challenge and sign it on one connection.

use anyhow::{anyhow, bail, Context};
use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{
    self, mailbox_request, media_request, node_request, profile_request, Status,
};
use dyapp_p2p_net::{build_swarm, replica_key, Behaviour, BehaviourEvent, Mode, REPLICAS};
use dyapp_profile::Profile;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::kad;
use libp2p::request_response::{Event, Message as RrMessage};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use prost::Message;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = match args.as_slice() {
        ["sign-profile"] => sign_profile(&Identity::generate(), 1),
        ["sign-profile", secret, version] => {
            let secret: [u8; 32] = unhex(secret)?.try_into().map_err(|_| anyhow!("secret must be 32 bytes"))?;
            sign_profile(&Identity::from_secret(&secret), version.parse()?)
        }
        ["heartbeat", addr, secret] => {
            let secret: [u8; 32] = unhex(secret)?.try_into().map_err(|_| anyhow!("secret must be 32 bytes"))?;
            let record = Identity::from_secret(&secret).sign(
                Domain::Heartbeat,
                proto::Heartbeat { time: chrono::Utc::now().timestamp().unsigned_abs() }.encode_to_vec(),
            );
            timeout(profile(addr, profile_request::Request::Heartbeat(record))).await?
        }
        ["info", addr] => timeout(info(addr)).await?,
        ["publish", addr, record] => timeout(publish(addr, record)).await?,
        ["get", addr, peer_id] => timeout(get(addr, peer_id)).await?,
        ["device-key"] => device_key(),
        ["put", addr, mailbox, ciphertext] => timeout(put(addr, mailbox, ciphertext, false)).await?,
        ["put-local", addr, mailbox, ciphertext] => timeout(put(addr, mailbox, ciphertext, true)).await?,
        ["fetch", addr, secret] => timeout(fetch(addr, secret)).await?,
        ["ack", addr, secret, ids @ ..] => timeout(ack(addr, secret, ids)).await?,
        ["closest", addr, key] => timeout(closest(addr, key)).await?,
        // Five lookups in a row, each waiting out dials to departed nodes.
        ["replicate", addr, peer_id, record] => {
            tokio::time::timeout(Duration::from_secs(60), replicate(addr, peer_id, record))
                .await
                .context("no reply in 60 s")??
        }
        ["flood", addr, n] => timeout(flood(addr, n.parse()?)).await?,
        ["watch", addr, secret] => timeout(watch(addr, secret)).await?,
        ["put-replicas", addr, mailbox] => {
            tokio::time::timeout(Duration::from_secs(60), put_replicas(addr, mailbox))
                .await
                .context("no reply in 60 s")??
        }
        ["media", addr, data] => timeout(media(addr, data, false)).await?,
        ["media-owned", addr, data] => timeout(media(addr, data, true)).await?,
        ["media-sized", addr, size, seed] => {
            let size: usize = size.parse()?;
            if size > dyapp_bootstrap::service::MAX_MEDIA_BYTES { bail!("blob exceeds 1 MiB"); }
            let seed = unhex(seed)?;
            if seed.len() != 32 { bail!("seed must be 32 bytes"); }
            let data: Vec<_> = seed.into_iter().cycle().take(size).collect();
            timeout(media(addr, &hex(&data), true)).await?
        }
        ["media-keep", addr, record] => {
            let record = SignedRecord::decode(unhex(record)?.as_slice())?;
            timeout(media_call(addr, media_request::Request::Keep(record))).await?
        }
        ["media-get", addr, hash] => {
            let get = proto::GetMedia { hash: unhex(hash)? };
            timeout(media_call(addr, media_request::Request::Get(get))).await?
        }
        ["turn", addr] => timeout(turn(addr)).await?,
        ["relays", addr] => timeout(relays(addr)).await?,
        _ => bail!("usage: test-peer sign-profile [secret version] | heartbeat <addr> <secret> | info <addr> | publish <addr> <hex> | get <addr> <peer_id> | device-key | put <addr> <mailbox> <hex> | put-local <addr> <mailbox> <hex> | fetch <addr> <secret> | ack <addr> <secret> <id>... | closest <addr> <key> | replicate <addr> <peer_id> <hex> | flood <addr> <n> | watch <addr> <secret> | put-replicas <addr> <mailbox> | media <addr> <hex> | media-owned <addr> <hex> | media-keep <addr> <record> | media-get <addr> <hash> | turn <addr> | relays <addr>"),
    };
    println!("{output}");
    Ok(())
}

fn sign_profile(identity: &Identity, version: u64) -> Value {
    let record = Profile {
        version,
        age: 26,
        ..Profile::default()
    }
    .sign(identity);
    json!({ "peer_id": identity.peer_id(), "record": hex(&record.encode_to_vec()), "secret": hex(&identity.secret()) })
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
        "peer_id": peer.to_string(),
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

async fn media(addr: &str, data: &str, owned: bool) -> anyhow::Result<Value> {
    let data = unhex(data)?;
    let owner = Identity::generate();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let keep = proto::MediaKeep {
        version: 1,
        hashes: vec![hash.clone()],
        time: chrono::Utc::now().timestamp().unsigned_abs(),
    };
    let record = owner.sign(Domain::MediaKeep, keep.encode_to_vec());
    let details = json!({ "owner": owner.peer_id(), "hash": hex(&hash), "keep_record": hex(&record.encode_to_vec()) });
    let steps = [
        media_request::Request::Keep(record),
        media_request::Request::Put(proto::MediaPut {
            owner: unhex(&owner.peer_id())?,
            data: data.clone(),
        }),
        media_request::Request::Get(proto::GetMedia { hash }),
    ];
    let (mut swarm, peer) = connect(addr).await?;
    let mut output = json!({});
    for (name, request) in ["keep", "put", "get"].into_iter().zip(steps) {
        let request = proto::MediaRequest {
            request: Some(request),
        };
        swarm.behaviour_mut().media.send_request(&peer, request);
        let response = wait(&mut swarm, |event| match event {
            BehaviourEvent::Media(event) => reply(event),
            _ => None,
        })
        .await?;
        let changed =
            name == "get" && response.status == i32::from(Status::Ok) && response.data != data;
        output[name] = if changed {
            "CHANGED"
        } else {
            status(response.status)
        }
        .into();
    }
    if owned {
        output["details"] = details;
    }
    Ok(output)
}

async fn media_call(addr: &str, request: media_request::Request) -> anyhow::Result<Value> {
    let (mut swarm, peer) = connect(addr).await?;
    swarm.behaviour_mut().media.send_request(
        &peer,
        proto::MediaRequest {
            request: Some(request),
        },
    );
    let response = wait(&mut swarm, |event| match event {
        BehaviourEvent::Media(event) => reply(event),
        _ => None,
    })
    .await?;
    Ok(json!({ "status": status(response.status), "data": hex(&response.data) }))
}

async fn turn(addr: &str) -> anyhow::Result<Value> {
    let (mut swarm, peer) = connect(addr).await?;
    let request = proto::NodeRequest {
        request: Some(node_request::Request::Turn(proto::TurnRequest {})),
    };
    swarm.behaviour_mut().node.send_request(&peer, request);
    let response = wait(&mut swarm, |event| match event {
        BehaviourEvent::Node(event) => reply(event),
        _ => None,
    })
    .await?;
    let turn = response.turn.unwrap_or_default();
    Ok(json!({
        "status": status(response.status),
        "username": turn.username,
        "password": turn.password,
        "urls": turn.urls,
        "expires": turn.expires,
    }))
}

async fn relays(addr: &str) -> anyhow::Result<Value> {
    let (mut swarm, _) = connect(addr).await?;
    let key = kad::RecordKey::new(&dyapp_p2p_net::TURN_KEY);
    swarm.behaviour_mut().kad.get_providers(key);
    let providers = wait(&mut swarm, |event| match event {
        BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
            result: kad::QueryResult::GetProviders(result),
            ..
        }) => Some(match result {
            // Every answer is a step, empty ones too: wait for providers or the end.
            Ok(kad::GetProvidersOk::FoundProviders { providers, .. }) if providers.is_empty() => {
                return None;
            }
            Ok(kad::GetProvidersOk::FoundProviders { providers, .. }) => Ok(providers),
            Ok(_) => Ok(Default::default()),
            Err(error) => Err(anyhow!("providers: {error}")),
        }),
        _ => None,
    })
    .await?;
    let providers: Vec<String> = providers.iter().map(PeerId::to_string).collect();
    Ok(json!({ "providers": providers }))
}

async fn closest(addr: &str, key: &str) -> anyhow::Result<Value> {
    let (mut swarm, _) = connect(addr).await?;
    let peers = lookup(&mut swarm, unhex(key)?).await?;
    let peers: Vec<String> = peers.iter().map(PeerId::to_string).collect();
    Ok(json!({ "peers": peers }))
}

async fn replicate(addr: &str, peer_id: &str, record: &str) -> anyhow::Result<Value> {
    let key = unhex(peer_id)?;
    let record = dyapp_identity::SignedRecord::decode(unhex(record)?.as_slice())?;
    let (mut swarm, _) = connect(addr).await?;
    let mut holders = Vec::new();
    for i in 0..REPLICAS {
        holders.push(closest_to(&mut swarm, replica_key(&key, i)).await?);
    }
    for holder in &holders {
        let request = proto::ProfileRequest {
            request: Some(profile_request::Request::Publish(record.clone())),
        };
        swarm.behaviour_mut().profile.send_request(holder, request);
        let response = wait(&mut swarm, |event| match event {
            BehaviourEvent::Profile(event) => reply(event),
            _ => None,
        })
        .await;
        if response
            .is_ok_and(|r| matches!(Status::try_from(r.status), Ok(Status::Ok | Status::Stale)))
        {
            let holders: Vec<_> = holders.iter().map(PeerId::to_string).collect();
            return Ok(json!({ "holders": holders }));
        }
    }
    bail!("no replica accepted the profile")
}

async fn flood(addr: &str, n: usize) -> anyhow::Result<Value> {
    let (mut swarm, peer) = connect(addr).await?;
    for _ in 0..n {
        let request = proto::ProfileRequest {
            request: Some(profile_request::Request::Get(proto::GetProfile {
                peer_id: vec![0; 32],
            })),
        };
        swarm.behaviour_mut().profile.send_request(&peer, request);
    }
    let mut counts = BTreeMap::new();
    for _ in 0..n {
        let reply = wait(&mut swarm, |event| match event {
            BehaviourEvent::Profile(event) => reply(event),
            _ => None,
        })
        .await;
        let name = reply.map_or("failed", |response| status(response.status));
        *counts.entry(name).or_insert(0u32) += 1;
    }
    Ok(json!(counts))
}

async fn closest_to(swarm: &mut Swarm<Behaviour>, key: Vec<u8>) -> anyhow::Result<PeerId> {
    let peers = lookup(swarm, key).await?;
    peers.first().copied().context("lookup found no node")
}

/// The nodes that answered a lookup of `key`, closest first; their addresses stay known to
/// `swarm` so requests reach them.
async fn lookup(swarm: &mut Swarm<Behaviour>, key: Vec<u8>) -> anyhow::Result<Vec<PeerId>> {
    swarm.behaviour_mut().kad.get_closest_peers(key);
    let found = wait(swarm, |event| match event {
        BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
            result: kad::QueryResult::GetClosestPeers(result),
            ..
        }) => Some(result.map_err(|error| anyhow!("lookup: {error}"))),
        _ => None,
    })
    .await?;
    for peer in &found.peers {
        for address in &peer.addrs {
            swarm.add_peer_address(peer.peer_id, address.clone());
        }
    }
    Ok(found.peers.into_iter().map(|peer| peer.peer_id).collect())
}

fn device_key() -> Value {
    let device = Identity::generate();
    let mailbox = dyapp_identity::key_hash(&device.public_key());
    json!({ "secret": hex(&device.secret()), "mailbox": hex(&mailbox) })
}

/// An envelope with a random id, signed by a fresh sender key.
fn envelope(mailbox: Vec<u8>, ciphertext: Vec<u8>) -> anyhow::Result<(Vec<u8>, SignedRecord)> {
    let mut id = [0u8; 16];
    getrandom::fill(&mut id).map_err(|e| anyhow!("random id: {e}"))?;
    let envelope = proto::Envelope {
        id: id.to_vec(),
        mailbox,
        ciphertext,
    };
    let record = Identity::generate().sign(Domain::Envelope, envelope.encode_to_vec());
    Ok((id.to_vec(), record))
}

async fn put(addr: &str, mailbox: &str, ciphertext: &str, replica: bool) -> anyhow::Result<Value> {
    let (id, record) = envelope(unhex(mailbox)?, unhex(ciphertext)?)?;
    let (mut swarm, peer) = connect(addr).await?;
    let request = if replica {
        mailbox_request::Request::ReplicaPut(proto::Envelopes {
            envelopes: vec![record],
        })
    } else {
        mailbox_request::Request::Put(record)
    };
    let response = mailbox_call(&mut swarm, peer, request).await?;
    Ok(json!({ "status": status(response.status), "id": hex(&id) }))
}

async fn put_replicas(addr: &str, mailbox: &str) -> anyhow::Result<Value> {
    let mailbox = unhex(mailbox)?;
    let (id, record) = envelope(mailbox.clone(), vec![1])?;
    let (mut swarm, _) = connect(addr).await?;
    let mut holders = Vec::new();
    for i in 0..REPLICAS {
        holders.push(closest_to(&mut swarm, replica_key(&mailbox, i)).await?);
    }
    for holder in &holders {
        let put = mailbox_request::Request::Put(record.clone());
        if mailbox_call(&mut swarm, *holder, put)
            .await
            .is_ok_and(|r| r.status == i32::from(Status::Ok))
        {
            let holders: Vec<_> = holders.iter().map(PeerId::to_string).collect();
            return Ok(json!({ "status": "STATUS_OK", "id": hex(&id), "holders": holders }));
        }
    }
    bail!("no replica accepted the envelope")
}

async fn watch(addr: &str, secret: &str) -> anyhow::Result<Value> {
    let (mut swarm, peer, device, nonce) = challenged(addr, secret).await?;
    let fetch = proto::Fetch {
        nonce,
        watch: true,
        ..proto::Fetch::default()
    };
    let record = device.sign(Domain::MailboxFetch, fetch.encode_to_vec());
    let response = mailbox_call(&mut swarm, peer, mailbox_request::Request::Fetch(record)).await?;
    let mailbox = hex(&dyapp_identity::key_hash(&device.public_key()));
    // The put runs on its own connection while this one waits for the push.
    let pushed = wait(&mut swarm, |event| match event {
        BehaviourEvent::Push(Event::Message {
            message: RrMessage::Request { request, .. },
            ..
        }) => Some(Ok(request.envelopes)),
        _ => None,
    });
    let (put, pushed) = tokio::join!(put(addr, &mailbox, "01", false), pushed);
    let ids = pushed?
        .iter()
        .map(|record| Ok(hex(&proto::Envelope::decode(record.payload.as_slice())?.id)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(json!({ "status": status(response.status), "id": put?["id"], "pushed": ids }))
}

async fn fetch(addr: &str, secret: &str) -> anyhow::Result<Value> {
    let (mut swarm, peer, device, nonce) = challenged(addr, secret).await?;
    let fetch = proto::Fetch {
        nonce,
        ..proto::Fetch::default()
    };
    let record = device.sign(Domain::MailboxFetch, fetch.encode_to_vec());
    let response = mailbox_call(&mut swarm, peer, mailbox_request::Request::Fetch(record)).await?;
    let ids = response
        .envelopes
        .iter()
        .map(|record| Ok(hex(&proto::Envelope::decode(record.payload.as_slice())?.id)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(json!({ "status": status(response.status), "ids": ids, "more": response.more }))
}

async fn ack(addr: &str, secret: &str, ids: &[&str]) -> anyhow::Result<Value> {
    let ids = ids
        .iter()
        .map(|id| unhex(id))
        .collect::<anyhow::Result<_>>()?;
    let (mut swarm, peer, device, nonce) = challenged(addr, secret).await?;
    let record = device.sign(
        Domain::MailboxAck,
        proto::Ack { nonce, ids }.encode_to_vec(),
    );
    let response = mailbox_call(&mut swarm, peer, mailbox_request::Request::Ack(record)).await?;
    Ok(json!({ "status": status(response.status) }))
}

/// Connects and gets a challenge nonce for the device key `secret` on that connection.
async fn challenged(
    addr: &str,
    secret: &str,
) -> anyhow::Result<(Swarm<Behaviour>, PeerId, Identity, Vec<u8>)> {
    let secret: [u8; 32] = unhex(secret)?
        .try_into()
        .map_err(|_| anyhow!("secret must be 32 bytes"))?;
    let (mut swarm, peer) = connect(addr).await?;
    let challenge = mailbox_request::Request::Challenge(proto::ChallengeRequest {});
    let response = mailbox_call(&mut swarm, peer, challenge).await?;
    Ok((swarm, peer, Identity::from_secret(&secret), response.nonce))
}

async fn mailbox_call(
    swarm: &mut Swarm<Behaviour>,
    peer: PeerId,
    request: mailbox_request::Request,
) -> anyhow::Result<proto::MailboxResponse> {
    let request = proto::MailboxRequest {
        request: Some(request),
    };
    swarm.behaviour_mut().mailbox.send_request(&peer, request);
    wait(swarm, |event| match event {
        BehaviourEvent::Mailbox(event) => reply(event),
        _ => None,
    })
    .await
}

/// Dials `addr` and returns the peer that answered: the address needs no `/p2p/` suffix.
async fn connect(addr: &str) -> anyhow::Result<(Swarm<Behaviour>, PeerId)> {
    let mut swarm = build_swarm(Keypair::generate_ed25519(), Mode::Client)?;
    let addr: Multiaddr = addr.parse()?;
    swarm.dial(addr.clone())?;
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                // A lookup starts from this node.
                swarm.behaviour_mut().kad.add_address(&peer_id, addr);
                return Ok((swarm, peer_id));
            }
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
