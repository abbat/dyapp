//! Client network core: one libp2p connection to a node and the profile, KeyPackage and mailbox
//! requests a client makes on it; [`chat`] delivers MLS chats through device mailboxes and
//! [`attachment`] encrypts their attachments.

pub mod attachment;
pub mod chat;

use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{self, key_package_request, mailbox_request, profile_request, Status};
use dyapp_p2p_net::{build_configured_swarm, Behaviour, BehaviourEvent, Mode, Protocols};
use dyapp_p2p_net::{SwarmOptions, REPLICAS};
use dyapp_profile::VerifiedProfile;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::request_response::{Event, Message as RrMessage};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use prost::Message as _;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a dial or one request may take.
pub const TIMEOUT: Duration = Duration::from_secs(15);
// Expiry of a published KeyPackage record, below the node's 30-day bound for clock skew.
const KEY_PACKAGE_RECORD_SECONDS: u64 = dyapp_messaging::mls::KEY_PACKAGE_LIFETIME_SECONDS - 3_600;

/// Why a client request failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The address could not be dialled.
    #[error("connect: {0}")]
    Connect(String),
    /// The connection or the request failed before a reply.
    #[error("request failed: {0}")]
    Request(String),
    /// No reply within [`TIMEOUT`].
    #[error("no reply in time")]
    Timeout,
    /// The node answered with a status other than OK.
    #[error("node replied {0:?}")]
    Status(Status),
    /// The reply or a record in it is malformed or wrongly signed.
    #[error("bad reply: {0}")]
    Reply(&'static str),
    /// A profile failed its checks.
    #[error(transparent)]
    Profile(#[from] dyapp_profile::Error),
    /// An MLS operation or incoming MLS message was refused.
    #[error(transparent)]
    Mls(#[from] dyapp_messaging::mls::Error),
    /// An attachment could not be sealed, or its blob failed a check.
    #[error("attachment: {0}")]
    Attachment(&'static str),
}

/// One fetched envelope: the id to ack and the MLS bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// 16 random bytes the sender chose.
    pub id: Vec<u8>,
    /// MLS message bytes.
    pub ciphertext: Vec<u8>,
}

/// A connection to one node.
pub struct Node {
    swarm: Swarm<Behaviour>,
    peer: PeerId,
}

impl Node {
    /// Dials `addr` with a fresh transport key and speaks `protocols`' network.
    pub async fn connect(addr: Multiaddr, protocols: Arc<Protocols>) -> Result<Self, Error> {
        let options = SwarmOptions {
            mode: Mode::Client,
            protocols,
            ..SwarmOptions::default()
        };
        let mut swarm = build_configured_swarm(Keypair::generate_ed25519(), options)
            .map_err(|error| Error::Connect(error.to_string()))?;
        swarm
            .dial(addr)
            .map_err(|error| Error::Connect(error.to_string()))?;
        timeout(async {
            loop {
                match swarm.select_next_some().await {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        return Ok(peer_id);
                    }
                    SwarmEvent::OutgoingConnectionError { error, .. } => {
                        return Err(Error::Connect(error.to_string()));
                    }
                    _ => {}
                }
            }
        })
        .await
        .map(|peer| Self { swarm, peer })
    }

    /// Publishes a signed profile.
    pub async fn publish_profile(&mut self, record: SignedRecord) -> Result<(), Error> {
        let response = self
            .profile(profile_request::Request::Publish(record))
            .await?;
        ok(response.status)
    }

    /// The owner's profile, checked; `peer_id` is its hex Peer ID.
    pub async fn get_profile(&mut self, peer_id: &str) -> Result<VerifiedProfile, Error> {
        let peer_id = unhex(peer_id)?;
        let response = self
            .profile(profile_request::Request::Get(proto::GetProfile {
                peer_id: peer_id.clone(),
            }))
            .await?;
        ok(response.status)?;
        let profile = dyapp_profile::verify(&response.record.ok_or(Error::Reply("no record"))?)?;
        if unhex(&profile.peer_id)? != peer_id {
            return Err(Error::Reply("profile of another owner"));
        }
        Ok(profile)
    }

    /// Publishes a KeyPackage of the device whose mailbox key is `device`, owned by `owner`
    /// (hex Peer ID).
    pub async fn publish_key_package(
        &mut self,
        device: &Identity,
        owner: &str,
        package: Vec<u8>,
        last_resort: bool,
        revision: u32,
    ) -> Result<(), Error> {
        let record = proto::KeyPackageRecord {
            owner: unhex(owner)?,
            mailbox: mailbox(&device.public_key()).to_vec(),
            package,
            expires: now() + KEY_PACKAGE_RECORD_SECONDS,
            last_resort,
            revision,
        };
        let signed = device.sign(Domain::KeyPackage, record.encode_to_vec());
        let response = self
            .keypackage(key_package_request::Request::Publish(signed))
            .await?;
        ok(response.status)
    }

    /// Takes one KeyPackage of the device with `mailbox` in `owner`'s profile, trying each
    /// allocation slot in turn; ordinary ones are consumed, a last-resort one stays.
    pub async fn fetch_key_package(
        &mut self,
        owner: &VerifiedProfile,
        mailbox: &[u8],
    ) -> Result<Vec<u8>, Error> {
        let mut last = Error::Status(Status::NotFound);
        for slot in 0..u32::from(REPLICAS) {
            let request = proto::GetKeyPackage {
                owner: unhex(&owner.peer_id)?,
                mailbox: mailbox.to_vec(),
                profile_version: owner.profile.version,
                slot,
            };
            let response = self
                .keypackage(key_package_request::Request::Fetch(request))
                .await?;
            if let Err(error) = ok(response.status) {
                last = error;
                continue;
            }
            let signed = response.record.ok_or(Error::Reply("no record"))?;
            let signer = signed
                .verify(Domain::KeyPackage)
                .map_err(|_| Error::Reply("bad KeyPackage signature"))?;
            let record = proto::KeyPackageRecord::decode(signed.payload.as_slice())
                .map_err(|_| Error::Reply("bad KeyPackage record"))?;
            if unhex(&signer)? != mailbox || record.mailbox != mailbox {
                return Err(Error::Reply("KeyPackage of another device"));
            }
            return Ok(record.package);
        }
        Err(last)
    }

    /// Stores `ciphertext` in `mailbox`, signed by a fresh sender key; returns the envelope id.
    pub async fn put(&mut self, mailbox: &[u8], ciphertext: Vec<u8>) -> Result<Vec<u8>, Error> {
        let mut id = [0u8; 16];
        getrandom::fill(&mut id).map_err(|error| Error::Request(error.to_string()))?;
        let envelope = proto::Envelope {
            id: id.to_vec(),
            mailbox: mailbox.to_vec(),
            ciphertext,
            ..Default::default()
        };
        let record = Identity::generate().sign(Domain::Envelope, envelope.encode_to_vec());
        let response = self.mailbox(mailbox_request::Request::Put(record)).await?;
        ok(response.status)?;
        Ok(id.to_vec())
    }

    /// The envelopes waiting in the mailbox of `device`, and whether more are waiting.
    pub async fn fetch(&mut self, device: &Identity) -> Result<(Vec<Envelope>, bool), Error> {
        let nonce = self.challenge().await?;
        let fetch = proto::Fetch {
            nonce,
            ..Default::default()
        };
        let record = device.sign(Domain::MailboxFetch, fetch.encode_to_vec());
        let response = self
            .mailbox(mailbox_request::Request::Fetch(record))
            .await?;
        ok(response.status)?;
        let envelopes = response
            .envelopes
            .iter()
            .map(|record| {
                let envelope = proto::Envelope::decode(record.payload.as_slice())
                    .map_err(|_| Error::Reply("bad envelope"))?;
                Ok(Envelope {
                    id: envelope.id,
                    ciphertext: envelope.ciphertext,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok((envelopes, response.more))
    }

    /// Deletes envelopes `ids` from the mailbox of `device`.
    pub async fn ack(&mut self, device: &Identity, ids: Vec<Vec<u8>>) -> Result<(), Error> {
        let nonce = self.challenge().await?;
        let record = device.sign(
            Domain::MailboxAck,
            proto::Ack { nonce, ids }.encode_to_vec(),
        );
        let response = self.mailbox(mailbox_request::Request::Ack(record)).await?;
        ok(response.status)
    }

    async fn challenge(&mut self) -> Result<Vec<u8>, Error> {
        let challenge = mailbox_request::Request::Challenge(proto::ChallengeRequest {});
        let response = self.mailbox(challenge).await?;
        ok(response.status)?;
        Ok(response.nonce)
    }

    async fn profile(
        &mut self,
        request: profile_request::Request,
    ) -> Result<proto::ProfileResponse, Error> {
        let request = proto::ProfileRequest {
            request: Some(request),
        };
        self.swarm
            .behaviour_mut()
            .profile
            .send_request(&self.peer, request);
        self.wait(|event| match event {
            BehaviourEvent::Profile(event) => reply(event),
            _ => None,
        })
        .await
    }

    async fn keypackage(
        &mut self,
        request: key_package_request::Request,
    ) -> Result<proto::KeyPackageResponse, Error> {
        let request = proto::KeyPackageRequest {
            request: Some(request),
        };
        self.swarm
            .behaviour_mut()
            .key_package
            .send_request(&self.peer, request);
        self.wait(|event| match event {
            BehaviourEvent::KeyPackage(event) => reply(event),
            _ => None,
        })
        .await
    }

    async fn mailbox(
        &mut self,
        request: mailbox_request::Request,
    ) -> Result<proto::MailboxResponse, Error> {
        let request = proto::MailboxRequest {
            request: Some(request),
        };
        self.swarm
            .behaviour_mut()
            .mailbox
            .send_request(&self.peer, request);
        self.wait(|event| match event {
            BehaviourEvent::Mailbox(event) => reply(event),
            _ => None,
        })
        .await
    }

    async fn wait<R>(
        &mut self,
        pick: impl Fn(BehaviourEvent) -> Option<Result<R, Error>>,
    ) -> Result<R, Error> {
        timeout(async {
            loop {
                match self.swarm.select_next_some().await {
                    SwarmEvent::Behaviour(event) => {
                        if let Some(result) = pick(event) {
                            return result;
                        }
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } if peer_id == self.peer => {
                        return Err(Error::Request("connection closed".into()));
                    }
                    _ => {}
                }
            }
        })
        .await
    }
}

/// The mailbox address of a device mailbox key.
pub fn mailbox(device_key: &[u8; 32]) -> [u8; 32] {
    dyapp_identity::key_hash(device_key)
}

async fn timeout<R>(request: impl Future<Output = Result<R, Error>>) -> Result<R, Error> {
    tokio::time::timeout(TIMEOUT, request)
        .await
        .map_err(|_| Error::Timeout)?
}

fn reply<Req, Resp>(event: Event<Req, Resp>) -> Option<Result<Resp, Error>> {
    match event {
        Event::Message {
            message: RrMessage::Response { response, .. },
            ..
        } => Some(Ok(response)),
        Event::OutboundFailure { error, .. } => Some(Err(Error::Request(error.to_string()))),
        _ => None,
    }
}

fn ok(status: i32) -> Result<(), Error> {
    match Status::try_from(status) {
        Ok(Status::Ok) => Ok(()),
        Ok(status) => Err(Error::Status(status)),
        Err(_) => Err(Error::Reply("unknown status")),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn unhex(text: &str) -> Result<Vec<u8>, Error> {
    if !text.len().is_multiple_of(2) {
        return Err(Error::Reply("odd-length hex"));
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            text.get(i..i + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or(Error::Reply("bad hex"))
        })
        .collect()
}
