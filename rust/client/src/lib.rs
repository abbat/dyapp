//! Client network core: one libp2p connection to a node and the profile, KeyPackage and mailbox
//! requests a client makes on it; [`chat`] delivers MLS chats through device mailboxes and
//! [`attachment`] encrypts their attachments; [`profiles`] keeps the newest profile per owner;
//! [`scores`] ranks nodes by the faults the client saw.

pub mod attachment;
pub mod chat;
pub mod interactions;
pub mod profiles;
pub mod scores;
pub mod session;
pub mod store;
pub mod tokens;

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
    /// A caller supplied a malformed mailbox address.
    #[error("bad client input: {0}")]
    Input(&'static str),
    /// A persisted outbound envelope is malformed or no longer verifies.
    #[error("bad prepared envelope")]
    PreparedEnvelope,
    /// A profile failed its checks.
    #[error(transparent)]
    Profile(#[from] dyapp_profile::Error),
    /// An MLS operation or incoming MLS message was refused.
    #[error(transparent)]
    Mls(#[from] dyapp_messaging::mls::Error),
    /// An attachment could not be sealed, or its blob failed a check.
    #[error("attachment: {0}")]
    Attachment(&'static str),
    /// A profile older than one already seen for its owner.
    #[error("profile version below the one held")]
    Rollback,
    /// A profile after its owner's tombstone; a deletion is final.
    #[error("profile deleted")]
    Deleted,
}

/// One fetched envelope: the id to ack and the MLS bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// 16 random bytes the sender chose.
    pub id: Vec<u8>,
    /// The one-off key that signed it: says nothing about the sender, who is inside the MLS bytes.
    pub sender_key: Vec<u8>,
    /// MLS message bytes.
    pub ciphertext: Vec<u8>,
}

/// A signed mailbox write that can be persisted and retried without changing its ID or bytes.
#[derive(Clone)]
pub struct PreparedEnvelope {
    mailbox: [u8; 32],
    id: [u8; 16],
    record: SignedRecord,
    bytes: Vec<u8>,
}

impl PreparedEnvelope {
    /// Reopen exact bytes saved before the first network attempt.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error> {
        let record = SignedRecord::decode(bytes.as_slice()).map_err(|_| Error::PreparedEnvelope)?;
        if record.encode_to_vec() != bytes || record.verify(Domain::Envelope).is_err() {
            return Err(Error::PreparedEnvelope);
        }
        let envelope = proto::Envelope::decode(record.payload.as_slice())
            .map_err(|_| Error::PreparedEnvelope)?;
        let mailbox = envelope
            .mailbox
            .try_into()
            .map_err(|_| Error::PreparedEnvelope)?;
        let id = envelope
            .id
            .try_into()
            .map_err(|_| Error::PreparedEnvelope)?;
        Ok(Self {
            mailbox,
            id,
            record,
            bytes,
        })
    }

    /// The exact signed record bytes for the durable outbox.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The destination device mailbox.
    pub fn mailbox(&self) -> &[u8; 32] {
        &self.mailbox
    }

    /// The stable envelope ID, also used for idempotent retry.
    pub fn id(&self) -> &[u8; 16] {
        &self.id
    }
}

impl std::fmt::Debug for PreparedEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedEnvelope")
            .field("bytes_len", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

/// Sign an ordinary MLS mailbox message with a fresh one-off key, before any network write.
pub fn prepare_put(mailbox: &[u8], ciphertext: Vec<u8>) -> Result<PreparedEnvelope, Error> {
    let mailbox: [u8; 32] = mailbox
        .try_into()
        .map_err(|_| Error::Input("mailbox must be 32 bytes"))?;
    let mut id = [0u8; 16];
    getrandom::fill(&mut id).map_err(|error| Error::Request(error.to_string()))?;
    let envelope = proto::Envelope {
        id: id.to_vec(),
        mailbox: mailbox.to_vec(),
        ciphertext,
        ..Default::default()
    };
    let record = Identity::generate().sign(Domain::Envelope, envelope.encode_to_vec());
    PreparedEnvelope::from_bytes(record.encode_to_vec())
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

    /// The node's Peer ID, the key of its [`scores::Scores`] entry.
    pub fn peer(&self) -> PeerId {
        self.peer
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

    /// Stores `ciphertext` in `mailbox`, signed by a fresh key used only for this envelope (sealed
    /// sender), never a published one; returns the envelope id.
    pub async fn put(&mut self, mailbox: &[u8], ciphertext: Vec<u8>) -> Result<Vec<u8>, Error> {
        let prepared = prepare_put(mailbox, ciphertext)?;
        self.put_prepared(&prepared).await?;
        Ok(prepared.id.to_vec())
    }

    /// Submit a previously prepared mailbox write; retries keep its exact ID and signed bytes.
    pub async fn put_prepared(&mut self, prepared: &PreparedEnvelope) -> Result<(), Error> {
        let response = self
            .mailbox(mailbox_request::Request::Put(prepared.record.clone()))
            .await?;
        ok(response.status)?;
        Ok(())
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
                    sender_key: record.public_key.clone(),
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

    /// Closes the mailbox of a device that the owner `identity` dropped from its profile, so
    /// nodes delete its envelopes and refuse its fetch and puts. `owner` is the binding the
    /// device signed at linking ([`owner_binding`]).
    pub async fn close_mailbox(
        &mut self,
        identity: &Identity,
        owner: SignedRecord,
    ) -> Result<(), Error> {
        let device_key: [u8; 32] = owner
            .public_key
            .as_slice()
            .try_into()
            .map_err(|_| Error::Reply("bad device key"))?;
        let revoke = proto::Revoke {
            mailbox: mailbox(&device_key).to_vec(),
            token_keys: Vec::new(),
            created_at: i64::try_from(now()).unwrap_or(i64::MAX),
            owner: Some(owner),
        };
        let record = identity.sign(Domain::MailboxRevoke, revoke.encode_to_vec());
        let response = self
            .mailbox(mailbox_request::Request::Revoke(record))
            .await?;
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
            age_seconds: 0,
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

/// The binding a device signs with its mailbox key `device_key` when it is linked: it accepts
/// the identity key `identity` as the owner that may later close its mailbox. Keep it on another
/// device of the owner, which sends it with [`Node::close_mailbox`].
pub fn owner_binding(device_key: &Identity, identity: &[u8; 32]) -> SignedRecord {
    device_key.sign(Domain::DeviceOwner, identity.to_vec())
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

#[cfg(test)]
mod tests {
    use super::{prepare_put, Error, Node, PreparedEnvelope};
    use dyapp_p2p_net::proto::{self, EnvelopeClass};
    use dyapp_p2p_net::{build_configured_swarm, BehaviourEvent, Protocols, SwarmOptions};
    use libp2p::futures::StreamExt;
    use libp2p::identity::Keypair;
    use libp2p::request_response::{Event as RequestEvent, Message as RequestMessage};
    use libp2p::swarm::SwarmEvent;
    use prost::Message as _;
    use std::sync::{Arc, Mutex};

    #[test]
    fn prepared_mailbox_write_reopens_with_exact_signed_bytes() {
        let mailbox = [7; 32];
        let prepared = prepare_put(&mailbox, vec![1, 2, 3]).unwrap();
        let bytes = prepared.as_bytes().to_vec();
        let reopened = PreparedEnvelope::from_bytes(bytes.clone()).unwrap();
        assert_eq!(reopened.as_bytes(), bytes);
        assert_eq!(reopened.mailbox(), &mailbox);
        assert_eq!(reopened.id(), prepared.id());
        let envelope = proto::Envelope::decode(reopened.record.payload.as_slice()).unwrap();
        assert_eq!(envelope.ciphertext, [1, 2, 3]);
        assert_eq!(envelope.class, EnvelopeClass::Message as i32);
        assert_ne!(
            prepare_put(&mailbox, vec![1, 2, 3]).unwrap().id(),
            prepared.id()
        );
        assert_eq!(
            format!("{prepared:?}"),
            format!("PreparedEnvelope {{ bytes_len: {}, .. }}", bytes.len())
        );
    }

    #[test]
    fn prepared_mailbox_write_rejects_wrong_address_and_tampering() {
        assert!(matches!(
            prepare_put(&[0; 31], vec![1]),
            Err(Error::Input(_))
        ));
        let prepared = prepare_put(&[0; 32], vec![1]).unwrap();
        let mut bytes = prepared.as_bytes().to_vec();
        *bytes.last_mut().unwrap() ^= 1;
        assert!(matches!(
            PreparedEnvelope::from_bytes(bytes),
            Err(Error::PreparedEnvelope)
        ));
    }

    #[tokio::test]
    async fn node_put_and_prepared_retry_reach_the_mailbox() {
        let mut server =
            build_configured_swarm(Keypair::generate_ed25519(), SwarmOptions::default()).unwrap();
        server
            .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .unwrap();
        let address = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                break address;
            }
        };
        let received = Arc::new(Mutex::new(Vec::new()));
        let copy = Arc::clone(&received);
        let task = tokio::spawn(async move {
            loop {
                if let SwarmEvent::Behaviour(BehaviourEvent::Mailbox(RequestEvent::Message {
                    message:
                        RequestMessage::Request {
                            request, channel, ..
                        },
                    ..
                })) = server.select_next_some().await
                {
                    let Some(proto::mailbox_request::Request::Put(record)) = request.request else {
                        panic!("expected mailbox put");
                    };
                    copy.lock().unwrap().push(record);
                    server
                        .behaviour_mut()
                        .mailbox
                        .send_response(
                            channel,
                            proto::MailboxResponse {
                                status: proto::Status::Ok.into(),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
        });
        let mut node = Node::connect(address, Arc::new(Protocols::public()))
            .await
            .unwrap();
        let mailbox = [7; 32];
        let prepared = prepare_put(&mailbox, vec![1]).unwrap();
        node.put_prepared(&prepared).await.unwrap();
        node.put_prepared(&prepared).await.unwrap();
        let fresh_id = node.put(&mailbox, vec![2]).await.unwrap();
        let records = received.lock().unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].encode_to_vec(), prepared.as_bytes());
        assert_eq!(records[1].encode_to_vec(), prepared.as_bytes());
        let fresh = proto::Envelope::decode(records[2].payload.as_slice()).unwrap();
        assert_eq!(fresh.id, fresh_id);
        assert_eq!(fresh.mailbox, mailbox);
        assert_eq!(fresh.ciphertext, [2]);
        task.abort();
    }
}
