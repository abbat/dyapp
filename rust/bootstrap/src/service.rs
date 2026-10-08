//! Answers node protocol requests (`/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`) from the
//! stores. The libp2p loop in `dyapp-node` passes each request here and sends back the reply.

use crate::{
    config::Role,
    rate_limit::{PeerRateLimiter, Traffic},
    BootstrapError, BootstrapStore, NodeConfig,
};
use dyapp_identity::{Domain, SignedRecord};
use dyapp_p2p_net::proto::{
    self, mailbox_request, node_request, profile_request, MailboxRequest, MailboxResponse,
    NodeInfo, NodeRequest, NodeResponse, ProfileRequest, ProfileResponse, Status,
};
use prost::Message;
use std::sync::atomic::{AtomicBool, Ordering};

/// Largest signed envelope payload a mailbox accepts.
pub const MAX_ENVELOPE_BYTES: usize = 100 * 1024;
/// Stored bytes per device mailbox.
pub const MAX_MAILBOX_BYTES: u64 = 10 * 1024 * 1024;
/// Envelope bytes per fetch reply, well under the 2 MiB protocol message limit.
const FETCH_BYTES: u64 = 1024 * 1024;
const MAX_FETCH: u32 = 100;

/// Traffic share (percent of the monthly cap) from which profile requests are shed; the mailbox
/// is shed only at the cap. ponytail: media and search, once served, are shed before profiles.
const SHED_PROFILES: u64 = 90;

pub struct Service {
    pub store: BootstrapStore,
    pub rate_limiter: PeerRateLimiter,
    pub traffic: Traffic,
    pub config: NodeConfig,
    /// Guards that refused the last request they checked: profiles full, messages full,
    /// profiles shed, mailbox shed. A change is logged once, not per request.
    tripped: [AtomicBool; 4],
}

impl Service {
    pub fn new(store: BootstrapStore, config: NodeConfig) -> Self {
        let cap = config.limits.monthly_traffic_gb.saturating_mul(1 << 30);
        Self {
            store,
            rate_limiter: PeerRateLimiter::new(config.limits.requests_per_second),
            traffic: Traffic::new(cap, config.storage.dir.join("traffic")),
            config,
            tripped: Default::default(),
        }
    }

    /// Logs when a guard starts or stops refusing; returns `tripped`.
    fn guard(&self, index: usize, tripped: bool, name: &str) -> bool {
        if self.tripped[index].swap(tripped, Ordering::Relaxed) != tripped {
            if tripped {
                tracing::warn!(guard = name, "limit reached, refusing requests");
            } else {
                tracing::info!(guard = name, "limit cleared");
            }
        }
        tripped
    }

    /// Whether a store is at its size limit or its file system at the free-space reserve.
    fn full(&self, index: usize, usage: (u64, u64), max_mb: u64, name: &str) -> bool {
        let (used, free) = usage;
        let reserve = self.config.limits.min_free_mb.saturating_mul(1 << 20);
        self.guard(
            index,
            used >= max_mb.saturating_mul(1 << 20) || free <= reserve,
            name,
        )
    }

    pub fn node(&self, request: NodeRequest) -> NodeResponse {
        self.traffic.add(request.encoded_len() as u64);
        match request.request {
            Some(node_request::Request::Info(_)) => NodeResponse {
                status: Status::Ok.into(),
                info: Some(self.info()),
            },
            None => NodeResponse {
                status: Status::Unsupported.into(),
                info: None,
            },
        }
    }

    fn info(&self) -> NodeInfo {
        let store = self.config.roles.contains(&Role::Store);
        NodeInfo {
            roles: if store {
                vec![proto::Role::Store.into()]
            } else {
                vec![]
            },
            max_profile_bytes: if store {
                dyapp_profile::MAX_PAYLOAD_LEN as u64
            } else {
                0
            },
            max_message_bytes: if store { MAX_ENVELOPE_BYTES as u64 } else { 0 },
            max_mailbox_bytes: if store { MAX_MAILBOX_BYTES } else { 0 },
            retention_seconds: if store { self.retention() } else { 0 },
            ..NodeInfo::default()
        }
    }

    fn retention(&self) -> u64 {
        u64::from(self.config.limits.message_ttl_hours) * 3600
    }

    /// `nonce` is the challenge issued on this libp2p connection: `challenge` sets it and the
    /// next `fetch` or `ack` takes it, valid or not, so a captured request cannot be replayed on
    /// this or another connection. Errors as in [`Service::profile`].
    pub fn mailbox(
        &self,
        peer: &str,
        nonce: &mut Option<[u8; 32]>,
        request: MailboxRequest,
    ) -> crate::Result<MailboxResponse> {
        if !self.config.roles.contains(&Role::Store) {
            return Ok(status(Status::Unsupported));
        }
        let used = self.traffic.add(request.encoded_len() as u64);
        if self.guard(3, used >= 100, "traffic: mailbox") || !self.rate_limiter.check_limit(peer) {
            return Ok(status(Status::RateLimited));
        }
        match request.request {
            Some(mailbox_request::Request::Challenge(_)) => {
                let mut fresh = [0; 32];
                getrandom::fill(&mut fresh)
                    .map_err(|e| BootstrapError::ServerError(e.to_string()))?;
                *nonce = Some(fresh);
                Ok(MailboxResponse {
                    nonce: fresh.to_vec(),
                    ..status(Status::Ok)
                })
            }
            Some(mailbox_request::Request::Put(record)) => self.put(&record),
            Some(mailbox_request::Request::Fetch(record)) => {
                let Ok((mailbox, fetch)) = owner_request::<proto::Fetch>(
                    &record,
                    Domain::MailboxFetch,
                    nonce.take(),
                    |f| &f.nonce,
                ) else {
                    return Ok(status(Status::Denied));
                };
                let limit = match fetch.limit {
                    0 => MAX_FETCH,
                    limit => limit.min(MAX_FETCH),
                };
                // ponytail: `watch` is ignored until /dyapp/mailbox-push is served.
                let (envelopes, more) = self.store.fetch_envelopes(&mailbox, limit, FETCH_BYTES)?;
                Ok(MailboxResponse {
                    envelopes,
                    more,
                    ..status(Status::Ok)
                })
            }
            Some(mailbox_request::Request::Ack(record)) => {
                let Ok((mailbox, ack)) =
                    owner_request::<proto::Ack>(&record, Domain::MailboxAck, nonce.take(), |a| {
                        &a.nonce
                    })
                else {
                    return Ok(status(Status::Denied));
                };
                if ack.ids.iter().any(|id| id.len() != 16) {
                    return Ok(status(Status::Invalid));
                }
                self.store.ack_envelopes(&mailbox, &ack.ids)?;
                Ok(status(Status::Ok))
            }
            None => Ok(status(Status::Unsupported)),
        }
    }

    fn put(&self, record: &SignedRecord) -> crate::Result<MailboxResponse> {
        if record.payload.len() > MAX_ENVELOPE_BYTES {
            return Ok(status(Status::TooLarge));
        }
        if record.verify(Domain::Envelope).is_err() {
            return Ok(status(Status::Denied));
        }
        let Ok(envelope) = proto::Envelope::decode(record.payload.as_slice()) else {
            return Ok(status(Status::Invalid));
        };
        if envelope.id.len() != 16 || envelope.mailbox.len() != 32 {
            return Ok(status(Status::Invalid));
        }
        let max_mb = self.config.limits.messages_max_mb;
        if self.full(1, self.store.messages_usage()?, max_mb, "disk: messages") {
            return Ok(status(Status::Full));
        }
        let expires_at = chrono::Utc::now().timestamp() + self.retention() as i64;
        let stored = self.store.put_envelope(
            &envelope.mailbox,
            &envelope.id,
            record,
            expires_at,
            MAX_MAILBOX_BYTES,
        )?;
        Ok(status(if stored { Status::Ok } else { Status::Full }))
    }

    /// `peer` is the remote libp2p peer, the key for rate limiting. A storage failure is an
    /// error: the caller drops the request and the client tries another node.
    pub fn profile(&self, peer: &str, request: ProfileRequest) -> crate::Result<ProfileResponse> {
        if !self.config.roles.contains(&Role::Store) {
            return Ok(reply(Status::Unsupported, None));
        }
        let used = self.traffic.add(request.encoded_len() as u64);
        if self.guard(2, used >= SHED_PROFILES, "traffic: profiles")
            || !self.rate_limiter.check_limit(peer)
        {
            return Ok(reply(Status::RateLimited, None));
        }
        match request.request {
            Some(profile_request::Request::Publish(record)) => self.publish(&record),
            Some(profile_request::Request::Get(get)) => {
                let Ok(peer_id) = <[u8; 32]>::try_from(get.peer_id.as_slice()) else {
                    return Ok(reply(Status::Invalid, None));
                };
                Ok(match self.store.get_profile(&hex(&peer_id))? {
                    Some(record) => reply(Status::Ok, Some(record)),
                    None => reply(Status::NotFound, None),
                })
            }
            None => Ok(reply(Status::Unsupported, None)),
        }
    }

    fn publish(&self, record: &SignedRecord) -> crate::Result<ProfileResponse> {
        if record.payload.len() > dyapp_profile::MAX_PAYLOAD_LEN {
            return Ok(reply(Status::TooLarge, None));
        }
        let max_mb = self.config.limits.profiles_max_mb;
        if self.full(0, self.store.profiles_usage()?, max_mb, "disk: profiles") {
            return Ok(reply(Status::Full, None));
        }
        match self.store.put_profile(record) {
            Ok(_) => Ok(reply(Status::Ok, None)),
            Err(BootstrapError::Profile(dyapp_profile::Error::Stale)) => {
                // A stale record verified, so its key is a valid 32-byte key.
                let key: [u8; 32] = record.public_key.as_slice().try_into().unwrap_or_default();
                let stored = self.store.get_profile(&dyapp_identity::peer_id(&key))?;
                Ok(reply(Status::Stale, stored))
            }
            Err(BootstrapError::Profile(dyapp_profile::Error::Signature(_))) => {
                Ok(reply(Status::Denied, None))
            }
            Err(BootstrapError::Profile(_)) => Ok(reply(Status::Invalid, None)),
            Err(error) => Err(error),
        }
    }
}

/// Checks a request signed by a mailbox's device key and carrying this connection's nonce;
/// returns the mailbox address, the hash of the signing key, and the decoded request.
fn owner_request<M: Message + Default>(
    record: &SignedRecord,
    domain: Domain,
    expected: Option<[u8; 32]>,
    nonce: fn(&M) -> &Vec<u8>,
) -> Result<([u8; 32], M), ()> {
    record.verify(domain).map_err(|_| ())?;
    let request = M::decode(record.payload.as_slice()).map_err(|_| ())?;
    if expected.is_none_or(|expected| nonce(&request).as_slice() != expected) {
        return Err(());
    }
    let key: [u8; 32] = record.public_key.as_slice().try_into().map_err(|_| ())?;
    Ok((dyapp_identity::key_hash(&key), request))
}

fn status(status: Status) -> MailboxResponse {
    MailboxResponse {
        status: status.into(),
        ..MailboxResponse::default()
    }
}

fn reply(status: Status, record: Option<SignedRecord>) -> ProfileResponse {
    ProfileResponse {
        status: status.into(),
        record,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;
    use dyapp_profile::Profile;

    fn service() -> Service {
        service_with(|_| {})
    }

    fn service_with(change: impl FnOnce(&mut crate::config::Limits)) -> Service {
        let dir = format!("/tmp/ai/test-service-{}", uuid::Uuid::new_v4());
        let mut config = NodeConfig::default();
        config.storage.dir = dir.clone().into();
        change(&mut config.limits);
        Service::new(BootstrapStore::new(&dir).unwrap(), config)
    }

    fn publish(record: SignedRecord) -> ProfileRequest {
        ProfileRequest {
            request: Some(profile_request::Request::Publish(record)),
        }
    }

    fn get(peer_id: Vec<u8>) -> ProfileRequest {
        ProfileRequest {
            request: Some(profile_request::Request::Get(proto::GetProfile { peer_id })),
        }
    }

    fn status(response: &ProfileResponse) -> Status {
        Status::try_from(response.status).unwrap()
    }

    #[test]
    fn publish_get_stale_and_errors() {
        let service = service();
        let identity = Identity::generate();
        let hex_id = identity.peer_id();
        let id: Vec<u8> = (0..hex_id.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex_id[i..i + 2], 16).unwrap())
            .collect();
        let v2 = Profile {
            version: 2,
            age: 30,
            ..Profile::default()
        }
        .sign(&identity);

        assert_eq!(
            status(&service.profile("p", get(id.clone())).unwrap()),
            Status::NotFound
        );
        assert_eq!(
            status(&service.profile("p", publish(v2.clone())).unwrap()),
            Status::Ok
        );
        let got = service.profile("p", get(id.clone())).unwrap();
        assert_eq!((status(&got), got.record), (Status::Ok, Some(v2.clone())));

        let v1 = Profile {
            version: 1,
            ..Profile::default()
        }
        .sign(&identity);
        let stale = service.profile("p", publish(v1)).unwrap();
        assert_eq!(
            (status(&stale), stale.record),
            (Status::Stale, Some(v2.clone()))
        );

        let mut forged = v2;
        forged.payload.push(0);
        assert_eq!(
            status(&service.profile("p", publish(forged)).unwrap()),
            Status::Denied
        );
        assert_eq!(
            status(&service.profile("p", get(vec![1; 5])).unwrap()),
            Status::Invalid
        );
        let empty = ProfileRequest { request: None };
        assert_eq!(
            status(&service.profile("p", empty).unwrap()),
            Status::Unsupported
        );
    }

    fn mailbox(
        service: &Service,
        nonce: &mut Option<[u8; 32]>,
        request: mailbox_request::Request,
    ) -> MailboxResponse {
        let request = MailboxRequest {
            request: Some(request),
        };
        service.mailbox("p", nonce, request).unwrap()
    }

    fn challenge(service: &Service, nonce: &mut Option<[u8; 32]>) -> Vec<u8> {
        let request = mailbox_request::Request::Challenge(proto::ChallengeRequest {});
        mailbox(service, nonce, request).nonce
    }

    fn fetch(device: &Identity, nonce: Vec<u8>) -> mailbox_request::Request {
        let fetch = proto::Fetch {
            nonce,
            ..proto::Fetch::default()
        };
        mailbox_request::Request::Fetch(device.sign(Domain::MailboxFetch, fetch.encode_to_vec()))
    }

    fn mailbox_status(response: &MailboxResponse) -> Status {
        Status::try_from(response.status).unwrap()
    }

    #[test]
    fn mailbox_put_fetch_ack_forged_and_replayed() {
        let service = service();
        let (device, sender, thief) = (
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
        );
        let address = dyapp_identity::key_hash(&device.public_key()).to_vec();
        let envelope = |id: Vec<u8>, ciphertext| proto::Envelope {
            id,
            mailbox: address.clone(),
            ciphertext,
        };
        let put = |envelope: proto::Envelope| {
            mailbox_request::Request::Put(sender.sign(Domain::Envelope, envelope.encode_to_vec()))
        };
        let (mut conn, mut other) = (None, None);

        let signed = sender.sign(
            Domain::Envelope,
            envelope(vec![7; 16], vec![1]).encode_to_vec(),
        );
        let response = mailbox(
            &service,
            &mut conn,
            mailbox_request::Request::Put(signed.clone()),
        );
        assert_eq!(mailbox_status(&response), Status::Ok);
        let mut forged = signed.clone();
        forged.payload.push(0);
        let response = mailbox(&service, &mut conn, mailbox_request::Request::Put(forged));
        assert_eq!(mailbox_status(&response), Status::Denied);
        let wrong_domain = sender.sign(Domain::Profile, signed.payload.clone());
        let response = mailbox(
            &service,
            &mut conn,
            mailbox_request::Request::Put(wrong_domain),
        );
        assert_eq!(mailbox_status(&response), Status::Denied);
        let response = mailbox(&service, &mut conn, put(envelope(vec![7; 5], vec![1])));
        assert_eq!(mailbox_status(&response), Status::Invalid);
        let large = envelope(vec![8; 16], vec![0; MAX_ENVELOPE_BYTES]);
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, put(large))),
            Status::TooLarge
        );

        // No challenge yet, then a nonce issued on another connection.
        let response = mailbox(&service, &mut conn, fetch(&device, vec![0; 32]));
        assert_eq!(mailbox_status(&response), Status::Denied);
        let foreign = challenge(&service, &mut other);
        let response = mailbox(&service, &mut conn, fetch(&device, foreign));
        assert_eq!(mailbox_status(&response), Status::Denied);

        // A fetch signed as an ack, then the real fetch and its replay.
        let nonce = challenge(&service, &mut conn);
        let ack_signed = proto::Fetch {
            nonce: nonce.clone(),
            ..proto::Fetch::default()
        };
        let cross = device.sign(Domain::MailboxAck, ack_signed.encode_to_vec());
        let response = mailbox(&service, &mut conn, mailbox_request::Request::Fetch(cross));
        assert_eq!(mailbox_status(&response), Status::Denied);
        let nonce = challenge(&service, &mut conn);
        let request = fetch(&device, nonce);
        let response = mailbox(&service, &mut conn, request.clone());
        assert_eq!(
            (mailbox_status(&response), response.envelopes, response.more),
            (Status::Ok, vec![signed], false)
        );
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, request)),
            Status::Denied
        );

        // Another key reads and acks only its own, empty mailbox.
        let nonce = challenge(&service, &mut conn);
        let response = mailbox(&service, &mut conn, fetch(&thief, nonce));
        assert_eq!(
            (mailbox_status(&response), response.envelopes.len()),
            (Status::Ok, 0)
        );
        let ack = |key: &Identity, nonce| {
            let ack = proto::Ack {
                nonce,
                ids: vec![vec![7; 16]],
            };
            mailbox_request::Request::Ack(key.sign(Domain::MailboxAck, ack.encode_to_vec()))
        };
        let nonce = challenge(&service, &mut conn);
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, ack(&thief, nonce))),
            Status::Ok
        );
        let nonce = challenge(&service, &mut conn);
        assert_eq!(
            mailbox(&service, &mut conn, fetch(&device, nonce))
                .envelopes
                .len(),
            1
        );

        let nonce = challenge(&service, &mut conn);
        let request = ack(&device, nonce);
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, request.clone())),
            Status::Ok
        );
        assert_eq!(
            mailbox_status(&mailbox(&service, &mut conn, request)),
            Status::Denied
        );
        let nonce = challenge(&service, &mut conn);
        assert!(mailbox(&service, &mut conn, fetch(&device, nonce))
            .envelopes
            .is_empty());
    }

    #[test]
    fn full_stores_and_traffic_cap_refuse_requests() {
        let identity = Identity::generate();
        let profile = || publish(Profile::default().sign(&identity));
        let envelope = proto::Envelope {
            id: vec![7; 16],
            mailbox: vec![1; 32],
            ciphertext: vec![1],
        };
        let put = mailbox_request::Request::Put(
            identity.sign(Domain::Envelope, envelope.encode_to_vec()),
        );

        let service = service_with(|l| (l.profiles_max_mb, l.messages_max_mb) = (0, 0));
        assert_eq!(
            status(&service.profile("p", profile()).unwrap()),
            Status::Full
        );
        let response = mailbox(&service, &mut None, put.clone());
        assert_eq!(mailbox_status(&response), Status::Full);
        let service = service_with(|l| l.min_free_mb = u64::MAX);
        assert_eq!(
            status(&service.profile("p", profile()).unwrap()),
            Status::Full
        );

        let service = service_with(|l| l.monthly_traffic_gb = 1);
        service.traffic.add((1 << 30) / 100 * 92);
        let limited = status(&service.profile("p", get(vec![0; 32])).unwrap());
        assert_eq!(limited, Status::RateLimited);
        let response = mailbox(&service, &mut None, put.clone());
        assert_eq!(
            mailbox_status(&response),
            Status::Ok,
            "mailbox shed before the cap"
        );
        service.traffic.add(1 << 30);
        let response = mailbox(&service, &mut None, put);
        assert_eq!(mailbox_status(&response), Status::RateLimited);
    }

    #[test]
    fn rate_limit_and_info() {
        let mut service = service();
        service.rate_limiter = PeerRateLimiter::new(1);
        let id = vec![0; 32];
        assert_eq!(
            status(&service.profile("p", get(id.clone())).unwrap()),
            Status::NotFound
        );
        assert_eq!(
            status(&service.profile("p", get(id.clone())).unwrap()),
            Status::RateLimited
        );
        assert_eq!(
            status(&service.profile("q", get(id)).unwrap()),
            Status::NotFound
        );

        let info = service.node(NodeRequest {
            request: Some(node_request::Request::Info(proto::InfoRequest {})),
        });
        assert_eq!(
            info.info.unwrap().roles,
            vec![i32::from(proto::Role::Store)]
        );
    }
}
