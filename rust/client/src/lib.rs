//! Client network core: one libp2p connection to a node and the profile, search, `KeyPackage` and
//! mailbox requests a client makes on it; [`chat`] delivers MLS chats through device mailboxes and
//! [`attachment`] encrypts their attachments; [`profiles`] keeps the newest profile per owner;
//! [`scores`] ranks nodes by the faults the client saw; [`compatibility`] compares verified
//! public profiles locally; [`places`] names and finds profile places.

pub mod attachment;
pub mod chat;
pub mod compatibility;
pub mod interactions;
/// The device link code and record.
pub mod link;
pub mod places;
mod prepared;
pub(crate) use prepared::prepare;
pub use prepared::{prepare_put, prepare_tokened_put, PreparedEnvelope};
pub mod profiles;
pub mod publish;
pub mod scores;
pub mod search;
pub mod seeds;
pub mod session;
pub mod sticker_manifest;
pub mod sticker_pack;
pub mod sticker_tar;
pub mod sticker_webp;
pub mod store;
pub mod tokens;
pub mod updates;

use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{
    self, key_package_request, mailbox_request, media_request, node_request, profile_request,
    search_request, Status,
};
use dyapp_p2p_net::{build_configured_swarm, Behaviour, BehaviourEvent, Mode, Protocols};
use dyapp_p2p_net::{id_has_pow, id_pow_bits};
use dyapp_p2p_net::{SwarmOptions, WireBytes, REPLICAS};
use dyapp_profile::VerifiedProfile;
use libp2p::core::ConnectedPoint;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::kad;
use libp2p::request_response::{Event, Message as RrMessage};
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use prost::Message as _;
use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a dial or one request may take.
pub const TIMEOUT: Duration = Duration::from_secs(15);
/// Hardest search puzzle a client solves, in leading zero bits; a node asking more is skipped.
pub const MAX_PUZZLE_BITS: u32 = 24;
// Expiry of a published KeyPackage record, below the node's 30-day bound for clock skew.
const KEY_PACKAGE_RECORD_SECONDS: u64 = dyapp_messaging::mls::KEY_PACKAGE_LIFETIME_SECONDS - 3_600;

/// A `KeyPackage` record of the device with mailbox key `device`, owned by `owner` (hex Peer ID).
fn sign_key_package(
    device: &Identity,
    owner: &str,
    package: Vec<u8>,
    last_resort: bool,
    revision: u32,
) -> Result<SignedRecord, Error> {
    let record = proto::KeyPackageRecord {
        owner: unhex(owner)?,
        mailbox: mailbox(&device.public_key()).to_vec(),
        package,
        expires: now() + KEY_PACKAGE_RECORD_SECONDS,
        last_resort,
        revision,
    };
    Ok(device.sign(Domain::KeyPackage, record.encode_to_vec()))
}

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
    /// A search node is busy; ask again after this long.
    #[error("rate limited for {0:?}")]
    RateLimited(Duration),
}

/// One fetched envelope: the id to ack and the MLS bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// 16 random bytes the sender chose.
    pub id: Vec<u8>,
    /// The one-off key that signed it, or the write token key the mailbox owner issued to a
    /// contact device; the sender's identity stays inside the MLS bytes.
    pub sender_key: Vec<u8>,
    /// MLS message bytes.
    pub ciphertext: Vec<u8>,
}

/// One bounded mailbox fetch from a single node.
/// A cursor is only valid for that node's Peer ID; callers persist that binding separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxPage {
    /// Envelopes in node order.
    pub envelopes: Vec<Envelope>,
    /// Whether the node reports another page.
    pub more: bool,
    /// Opaque position for the next fetch on the same node.
    pub next_cursor: Option<Vec<u8>>,
}

static WIRE: OnceLock<Arc<WireBytes>> = OnceLock::new();

/// Payload counter shared by every client swarm of this process.
fn wire() -> Arc<WireBytes> {
    WIRE.get_or_init(Arc::default).clone()
}

/// `[in, out]` payload bytes of every client swarm since the first one started this process;
/// `None` before that. Excludes transport, encryption and gossip control traffic.
#[must_use]
pub fn traffic() -> Option<[u64; 2]> {
    let slots = WIRE.get()?.totals().0;
    Some(slots.iter().fold([0, 0], |[i, o], [a, b]| [i + a, o + b]))
}

/// A connection to one node.
pub struct Node {
    swarm: Swarm<Behaviour>,
    peer: PeerId,
}

impl Node {
    /// Dials `addr` with a fresh transport key and speaks `protocols`' network.
    ///
    /// # Errors
    /// Returns [`Error::Connect`] for swarm setup or dial failure, or
    /// [`Error::Timeout`] if no connection is established before the deadline.
    pub async fn connect(addr: Multiaddr, protocols: Arc<Protocols>) -> Result<Self, Error> {
        let options = SwarmOptions {
            mode: Mode::Client,
            protocols,
            wire: wire(),
            ..SwarmOptions::default()
        };
        let mut swarm = build_configured_swarm(Keypair::generate_ed25519(), options)
            .map_err(|error| Error::Connect(error.to_string()))?;
        swarm
            .dial(addr.clone())
            .map_err(|error| Error::Connect(error.to_string()))?;
        timeout(async {
            loop {
                match swarm.select_next_some().await {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        // A DHT lookup starts from this node.
                        swarm.behaviour_mut().kad.add_address(&peer_id, addr);
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
    #[must_use]
    pub fn peer(&self) -> PeerId {
        self.peer
    }

    /// Addresses of up to `max` nodes providing `key` in the DHT
    /// ([`dyapp_p2p_net::SEARCH_KEY`], [`dyapp_p2p_net::TURN_KEY`]) for [`Node::connect`], in no
    /// set order; fewer when the lookup or [`TIMEOUT`] ends first. Each is dialed once to learn
    /// a working address, since the DHT keeps a provider's addresses only during the lookup.
    pub async fn providers(&mut self, key: &[u8], max: usize) -> Vec<Multiaddr> {
        let swarm = &mut self.swarm;
        let query = swarm
            .behaviour_mut()
            .kad
            .get_providers(kad::RecordKey::new(&key));
        let (mut found, mut seen, mut dials, mut closing, mut done) = (
            Vec::new(),
            HashSet::new(),
            HashSet::new(),
            HashSet::new(),
            false,
        );
        let lookup = async {
            // Kademlia may send the next lookup down a closing connection, where it goes
            // unanswered: return only once the swarm has dropped them.
            while (found.len() < max && !(done && dials.is_empty())) || !closing.is_empty() {
                match swarm.select_next_some().await {
                    SwarmEvent::Behaviour(BehaviourEvent::Kad(
                        kad::Event::OutboundQueryProgressed {
                            id,
                            result: kad::QueryResult::GetProviders(result),
                            step,
                            ..
                        },
                    )) if id == query => {
                        if let Ok(kad::GetProvidersOk::FoundProviders { providers, .. }) = result {
                            for provider in providers {
                                if !seen.insert(provider) || !id_has_pow(&provider, id_pow_bits()) {
                                    continue;
                                }
                                // A second connection even to the node we are on: its address.
                                let dial = DialOpts::peer_id(provider)
                                    .condition(PeerCondition::Always)
                                    .build();
                                let connection = dial.connection_id();
                                if swarm.dial(dial).is_ok() {
                                    dials.insert(connection);
                                }
                            }
                        }
                        done |= step.last;
                    }
                    SwarmEvent::ConnectionEstablished {
                        connection_id,
                        endpoint: ConnectedPoint::Dialer { address, .. },
                        ..
                    } if dials.remove(&connection_id) => {
                        swarm.close_connection(connection_id);
                        closing.insert(connection_id);
                        found.push(address);
                    }
                    SwarmEvent::ConnectionClosed { connection_id, .. } => {
                        closing.remove(&connection_id);
                    }
                    SwarmEvent::OutgoingConnectionError { connection_id, .. } => {
                        dials.remove(&connection_id);
                    }
                    _ => {}
                }
            }
        };
        let _ = tokio::time::timeout(TIMEOUT, lookup).await;
        // Dials may still connect while the others close.
        found.truncate(max);
        found
    }

    /// Publishes a signed profile.
    ///
    /// # Errors
    /// Returns request/deadline errors, [`Error::Status`] for rejection, or
    /// [`Error::Reply`] for an unknown status.
    pub async fn publish_profile(&mut self, record: SignedRecord) -> Result<(), Error> {
        let response = self
            .profile(profile_request::Request::Publish(record))
            .await?;
        ok(response.status)
    }

    /// The owner's profile, checked; `peer_id` is its hex Peer ID.
    ///
    /// # Errors
    /// Returns [`Error::Input`] for an invalid Peer ID, request/deadline/status
    /// errors, [`Error::Profile`] for invalid profile content, or [`Error::Reply`]
    /// for a missing record, unknown status or mismatched owner.
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

    /// Publishes a `KeyPackage` of the device whose mailbox key is `device`, owned by `owner`
    /// (hex Peer ID).
    ///
    /// # Errors
    /// Returns [`Error::Input`] for an invalid owner Peer ID, or the request,
    /// deadline, status and reply errors of [`Self::publish_signed_key_package`].
    pub async fn publish_key_package(
        &mut self,
        device: &Identity,
        owner: &str,
        package: Vec<u8>,
        last_resort: bool,
        revision: u32,
    ) -> Result<(), Error> {
        let signed = sign_key_package(device, owner, package, last_resort, revision)?;
        self.publish_signed_key_package(signed).await
    }

    /// Publishes a signed `KeyPackage` record; the node takes a repeat of a stored one as done.
    ///
    /// # Errors
    /// Returns request/deadline errors, [`Error::Status`] for rejection, or
    /// [`Error::Reply`] for an unknown status.
    pub async fn publish_signed_key_package(&mut self, signed: SignedRecord) -> Result<(), Error> {
        let response = self
            .keypackage(key_package_request::Request::Publish(signed))
            .await?;
        ok(response.status)
    }

    /// Takes one `KeyPackage` of the device with `mailbox` in `owner`'s profile, trying each
    /// allocation slot in turn; ordinary ones are consumed, a last-resort one stays.
    ///
    /// # Errors
    /// Returns [`Error::Input`] for an invalid owner Peer ID, request/deadline
    /// errors, the last rejected slot's status, or [`Error::Reply`] for missing,
    /// malformed, wrongly signed or wrong-device records.
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
    ///
    /// # Errors
    /// Returns preparation errors from [`prepare_put`] or submission errors from
    /// [`Self::put_prepared`].
    pub async fn put(&mut self, mailbox: &[u8], ciphertext: Vec<u8>) -> Result<Vec<u8>, Error> {
        let prepared = prepare_put(mailbox, ciphertext)?;
        self.put_prepared(&prepared).await?;
        Ok(prepared.id().to_vec())
    }

    /// Submit a previously prepared mailbox write; retries keep its exact ID and signed bytes.
    ///
    /// # Errors
    /// Returns request/deadline/status errors, or [`Error::Reply`] for an unknown status.
    pub async fn put_prepared(&mut self, prepared: &PreparedEnvelope) -> Result<(), Error> {
        let response = self
            .mailbox(mailbox_request::Request::Put(prepared.record.clone()))
            .await?;
        ok(response.status)?;
        Ok(())
    }

    /// The envelopes waiting in the mailbox of `device`, and whether more are waiting.
    /// Existing first-page callers can use this API without handling a cursor.
    ///
    /// # Errors
    /// Returns request/deadline/status errors or [`Error::Reply`] for malformed
    /// challenge, status or envelope data.
    pub async fn fetch(&mut self, device: &Identity) -> Result<(Vec<Envelope>, bool), Error> {
        let page = self.fetch_inner(device, None, 0, false).await?;
        Ok((page.envelopes, page.more))
    }

    /// Fetch one bounded page from this node. `cursor` must come from this same node's Peer ID.
    /// An old or malformed node that claims more without advancing is rejected.
    ///
    /// # Errors
    /// Returns [`Error::Input`] for an invalid limit or cursor, request/deadline/status
    /// errors, or [`Error::Reply`] for malformed data or a cursor that does not advance.
    pub async fn fetch_page(
        &mut self,
        device: &Identity,
        cursor: Option<&[u8]>,
        limit: u32,
    ) -> Result<MailboxPage, Error> {
        if !(1..=100).contains(&limit) || cursor.is_some_and(|value| value.is_empty()) {
            return Err(Error::Input("bad mailbox page limit or cursor"));
        }
        self.fetch_inner(device, cursor, limit, true).await
    }

    async fn fetch_inner(
        &mut self,
        device: &Identity,
        cursor: Option<&[u8]>,
        limit: u32,
        require_progress: bool,
    ) -> Result<MailboxPage, Error> {
        let nonce = self.challenge().await?;
        let fetch = proto::Fetch {
            nonce,
            limit,
            cursor: cursor.unwrap_or_default().to_vec(),
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
        if require_progress
            && response.more
            && (response.next_cursor.is_empty()
                || cursor.is_some_and(|previous| previous == response.next_cursor.as_slice()))
        {
            return Err(Error::Reply("mailbox cursor did not advance"));
        }
        Ok(MailboxPage {
            envelopes,
            more: response.more,
            next_cursor: (!response.next_cursor.is_empty()).then_some(response.next_cursor),
        })
    }

    /// Deletes envelopes `ids` from the mailbox of `device`.
    ///
    /// # Errors
    /// Returns request/deadline/status errors or [`Error::Reply`] for malformed
    /// challenge data or an unknown status.
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
    ///
    /// # Errors
    /// Returns [`Error::Reply`] for an invalid device key or unknown status,
    /// or request/deadline/status errors.
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

    /// Revokes write token `keys` of the mailbox of `device`, so nodes delete their envelopes and
    /// refuse their puts. Sends one revoke per 64 keys, the most a node takes.
    ///
    /// # Errors
    /// Returns request/deadline/status errors or [`Error::Reply`] for an unknown
    /// status. Earlier chunks may already have been revoked when a later chunk fails.
    pub async fn revoke_tokens(
        &mut self,
        device: &Identity,
        keys: &[[u8; 32]],
    ) -> Result<(), Error> {
        for chunk in keys.chunks(64) {
            let revoke = proto::Revoke {
                mailbox: mailbox(&device.public_key()).to_vec(),
                token_keys: chunk.iter().map(|key| key.to_vec()).collect(),
                created_at: i64::try_from(now()).unwrap_or(i64::MAX),
                owner: None,
            };
            let record = device.sign(Domain::MailboxRevoke, revoke.encode_to_vec());
            let response = self
                .mailbox(mailbox_request::Request::Revoke(record))
                .await?;
            ok(response.status)?;
        }
        Ok(())
    }

    /// Replaces `owner`'s complete blob list on this node with `hashes` at `version`; returns
    /// the hashes the node could not recover or check.
    ///
    /// # Errors
    /// Returns request/deadline/status errors or [`Error::Reply`] for an unknown status.
    pub async fn keep_media(
        &mut self,
        owner: &Identity,
        version: u64,
        hashes: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, Error> {
        let keep = proto::MediaKeep {
            version,
            hashes,
            time: now(),
        };
        let record = owner.sign(Domain::MediaKeep, keep.encode_to_vec());
        let response = self.media(media_request::Request::Keep(record)).await?;
        ok(response.status)?;
        Ok(response.missing)
    }

    /// Uploads a blob that a keep or attachment of `owner` (hex Peer ID) lists on this node.
    ///
    /// # Errors
    /// Returns [`Error::Input`] for an invalid owner Peer ID, request/deadline/status
    /// errors, or [`Error::Reply`] for an unknown status.
    pub async fn put_media(&mut self, owner: &str, data: Vec<u8>) -> Result<(), Error> {
        let put = proto::MediaPut {
            owner: unhex(owner)?,
            data,
        };
        let response = self.media(media_request::Request::Put(put)).await?;
        ok(response.status)
    }

    /// Announces the blob of `attachment`, signed by `sender`, so this node accepts its put;
    /// returns the hashes the node does not hold yet.
    ///
    /// # Errors
    /// Returns request/deadline/status errors or [`Error::Reply`] for an unknown status.
    pub async fn attach_media(
        &mut self,
        sender: &Identity,
        attachment: &attachment::Attachment,
    ) -> Result<Vec<Vec<u8>>, Error> {
        let attach = proto::MediaAttach {
            release: attachment.release_hash().to_vec(),
            hashes: vec![attachment.blob_sha256.to_vec()],
            created: now(),
        };
        let record = sender.sign(Domain::MediaAttach, attach.encode_to_vec());
        let response = self.media(media_request::Request::Attach(record)).await?;
        ok(response.status)?;
        Ok(response.missing)
    }

    /// The whole blob whose SHA-256 is `hash`, checked against it.
    // shortcut: one request for the whole blob, at most 6 MiB; read pieces once blobs grow.
    ///
    /// # Errors
    /// Returns request/deadline/status errors or [`Error::Reply`] for an unknown
    /// status or a blob whose hash differs from `hash`.
    pub async fn get_media(&mut self, hash: &[u8; 32]) -> Result<Vec<u8>, Error> {
        let get = proto::GetMedia {
            hash: hash.to_vec(),
            ..Default::default()
        };
        let response = self.media(media_request::Request::Get(get)).await?;
        ok(response.status)?;
        if dyapp_identity::sha256(&response.data) != *hash {
            return Err(Error::Reply("blob of another hash"));
        }
        Ok(response.data)
    }

    /// Drops the blobs of the attachment released by `secret` from this node.
    ///
    /// # Errors
    /// Returns request/deadline/status errors or [`Error::Reply`] for an unknown status.
    pub async fn release_media(&mut self, secret: &[u8; 32]) -> Result<(), Error> {
        let release = proto::MediaRelease {
            secret: secret.to_vec(),
        };
        let response = self.media(media_request::Request::Release(release)).await?;
        ok(response.status)
    }

    async fn challenge(&mut self) -> Result<Vec<u8>, Error> {
        let challenge = mailbox_request::Request::Challenge(proto::ChallengeRequest {});
        let response = self.mailbox(challenge).await?;
        ok(response.status)?;
        Ok(response.nonce)
    }

    /// Search index hits (key hash, version) for `conditions`, at most `limit` (0 means 50,
    /// at most 200), with the conditions the node applied.
    ///
    /// # Errors
    /// Returns request/deadline/status/rate-limit errors, or [`Error::Reply`] for
    /// an unknown status, malformed puzzle challenge or excessive puzzle difficulty.
    pub async fn search_query(
        &mut self,
        conditions: Vec<proto::Condition>,
        limit: u32,
    ) -> Result<proto::SearchResponse, Error> {
        self.search_puzzled(|puzzle| {
            search_request::Request::Query(proto::SearchQuery {
                conditions: conditions.clone(),
                limit,
                puzzle,
            })
        })
        .await
    }

    /// Signed search profiles of up to 50 key hashes; `partial` when the node cut the
    /// reply at 1 MiB, so the keys it left out need another request.
    ///
    /// # Errors
    /// Returns the request, deadline, status, rate-limit and puzzle errors of
    /// [`Self::search_query`].
    pub async fn search_get(&mut self, keys: Vec<Vec<u8>>) -> Result<proto::SearchResponse, Error> {
        self.search_puzzled(|puzzle| {
            search_request::Request::Get(proto::SearchGet {
                keys: keys.clone(),
                puzzle,
            })
        })
        .await
    }

    /// Publishes a signed profile to the search index.
    ///
    /// # Errors
    /// Returns request/deadline/status/rate-limit errors, or [`Error::Reply`] for an unknown status.
    pub async fn search_publish(&mut self, record: SignedRecord) -> Result<(), Error> {
        search_ok(
            self.search(search_request::Request::Publish(record))
                .await?,
        )
        .map(drop)
    }

    /// Sends a signed heartbeat that keeps the owner's search profile listed.
    ///
    /// # Errors
    /// Returns request/deadline/status/rate-limit errors, or [`Error::Reply`] for an unknown status.
    pub async fn search_heartbeat(&mut self, record: SignedRecord) -> Result<(), Error> {
        search_ok(
            self.search(search_request::Request::Heartbeat(record))
                .await?,
        )
        .map(drop)
    }

    /// TURN credentials for a call's ICE servers from a node with the turn role; find such
    /// nodes with [`Self::providers`] on [`dyapp_p2p_net::TURN_KEY`]. Ask again before `expires`.
    ///
    /// # Errors
    /// Returns request/deadline/status errors, or [`Error::Reply`] for an unknown
    /// status or missing TURN credentials.
    pub async fn turn(&mut self) -> Result<proto::TurnCredentials, Error> {
        let response = self
            .node(node_request::Request::Turn(proto::TurnRequest {}))
            .await?;
        response.turn.ok_or(Error::Reply("no TURN credentials"))
    }

    /// The node's roles and limits; each value is a hint from an untrusted node.
    ///
    /// # Errors
    /// Returns request/deadline/status errors, or [`Error::Reply`] for an unknown
    /// status or missing node information.
    pub async fn info(&mut self) -> Result<proto::NodeInfo, Error> {
        let response = self
            .node(node_request::Request::Info(proto::InfoRequest {}))
            .await?;
        response.info.ok_or(Error::Reply("no node info"))
    }

    async fn node(&mut self, request: node_request::Request) -> Result<proto::NodeResponse, Error> {
        let request = proto::NodeRequest {
            request: Some(request),
        };
        self.swarm
            .behaviour_mut()
            .node
            .send_request(&self.peer, request);
        let response = self
            .wait(|event| match event {
                BehaviourEvent::Node(event) => reply(event),
                _ => None,
            })
            .await?;
        ok(response.status)?;
        Ok(response)
    }

    // Sends `request` without a puzzle, and once more with a solution when the node asks.
    async fn search_puzzled(
        &mut self,
        request: impl Fn(Option<proto::Puzzle>) -> search_request::Request,
    ) -> Result<proto::SearchResponse, Error> {
        let mut response = self.search(request(None)).await?;
        if response.status == i32::from(Status::Puzzle) {
            if response.puzzle_bits > MAX_PUZZLE_BITS {
                return Err(Error::Reply("puzzle too hard"));
            }
            let challenge: [u8; 16] = response
                .puzzle_challenge
                .as_slice()
                .try_into()
                .map_err(|_| Error::Reply("bad puzzle challenge"))?;
            // shortcut: solves on the calling task (under a second at 20 bits); move to a
            // blocking thread if the UI thread ever awaits this.
            let peer = self.swarm.local_peer_id().to_base58();
            let nonce = dyapp_p2p_net::puzzle::solve(&challenge, &peer, response.puzzle_bits);
            let puzzle = proto::Puzzle {
                challenge: challenge.to_vec(),
                nonce,
            };
            response = self.search(request(Some(puzzle))).await?;
        }
        search_ok(response)
    }

    async fn search(
        &mut self,
        request: search_request::Request,
    ) -> Result<proto::SearchResponse, Error> {
        let request = proto::SearchRequest {
            request: Some(request),
        };
        self.swarm
            .behaviour_mut()
            .search
            .send_request(&self.peer, request);
        self.wait(|event| match event {
            BehaviourEvent::Search(event) => reply(event),
            _ => None,
        })
        .await
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

    async fn media(
        &mut self,
        request: media_request::Request,
    ) -> Result<proto::MediaResponse, Error> {
        let request = proto::MediaRequest {
            request: Some(request),
        };
        self.swarm
            .behaviour_mut()
            .media
            .send_request(&self.peer, request);
        self.wait(|event| match event {
            BehaviourEvent::Media(event) => reply(event),
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
#[must_use]
pub fn mailbox(device_key: &[u8; 32]) -> [u8; 32] {
    dyapp_identity::key_hash(device_key)
}

/// The binding a device signs with its mailbox key `device_key` when it is linked: it accepts
/// the identity key `identity` as the owner that may later close its mailbox. The owner's profile
/// carries it ([`publish::list_device`]), so another own device can send it with
/// [`Node::close_mailbox`].
#[must_use]
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

fn search_ok(response: proto::SearchResponse) -> Result<proto::SearchResponse, Error> {
    if response.status == i32::from(Status::RateLimited) {
        return Err(Error::RateLimited(Duration::from_secs(
            response.retry_after,
        )));
    }
    ok(response.status).map(|()| response)
}

/// Unix seconds.
#[must_use]
pub fn now() -> u64 {
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
mod tests;
