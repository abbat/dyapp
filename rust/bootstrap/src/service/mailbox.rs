//! The mailbox requests: put, fetch, ack and node-to-node repair.

use super::*;
use crate::storage::Put;

impl Service {
    /// `nonce` is the challenge issued on this libp2p connection: `challenge` sets it and the
    /// next `fetch` or `ack` takes it, valid or not, so a captured request cannot be replayed on
    /// this or another connection. Errors as in [`Service::profile`].
    pub fn mailbox(
        &self,
        peer: &Peer,
        nonce: &mut Option<[u8; 32]>,
        request: MailboxRequest,
    ) -> crate::Result<MailboxResponse> {
        self.mailbox_inner(peer, nonce, request, true)
    }

    /// Validates and bills a client put before the swarm looks up its holders; stores nothing.
    pub fn prepare_mailbox(
        &self,
        peer: &Peer,
        record: SignedRecord,
    ) -> crate::Result<MailboxResponse> {
        self.mailbox_inner(
            peer,
            &mut None,
            MailboxRequest {
                request: Some(mailbox_request::Request::Put(record)),
            },
            false,
        )
    }

    fn mailbox_inner(
        &self,
        peer: &Peer,
        nonce: &mut Option<[u8; 32]>,
        request: MailboxRequest,
        store: bool,
    ) -> crate::Result<MailboxResponse> {
        if !self.config.roles.contains(&Role::Store) {
            return Ok(status(Status::Unsupported));
        }
        let is_replica = matches!(
            request.request,
            Some(mailbox_request::Request::ReplicaPut(_))
        );
        // Inventory is node-to-node repair: refused before it reads a mailbox.
        let node_only = is_replica
            || matches!(
                request.request,
                Some(mailbox_request::Request::Inventory(_))
            );
        if node_only && !self.node_peer(peer) {
            self.strike(peer);
            return Ok(status(Status::Denied));
        }
        let units = if matches!(request.request, Some(mailbox_request::Request::Put(_))) {
            5
        } else {
            1
        };
        if self.shed(request.encoded_len(), 100)
            || !if is_replica {
                self.admit_quiet(peer, units)
            } else {
                self.admit_units(peer, units)
            }
        {
            return Ok(status(Status::RateLimited));
        }
        if self.refused(peer) {
            return Ok(status(Status::Refused));
        }
        let mut strike = true;
        let response = self.mailbox_request(peer, nonce, request, store, &mut strike)?;
        if strike && response.status == i32::from(Status::Denied) {
            self.strike(peer);
        }
        Ok(response)
    }

    /// `strike` is cleared when a DENIED answer is no sign of abuse.
    fn mailbox_request(
        &self,
        peer: &Peer,
        nonce: &mut Option<[u8; 32]>,
        request: MailboxRequest,
        store: bool,
        strike: &mut bool,
    ) -> crate::Result<MailboxResponse> {
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
            Some(mailbox_request::Request::Put(record)) => {
                let group = Group::Ip(&peer.group);
                let (response, strikes) = self.put_inner(
                    &record,
                    store,
                    Source::Client,
                    MAX_ENVELOPE_BYTES,
                    &peer.id,
                    group,
                )?;
                *strike = strikes;
                Ok(response)
            }
            Some(mailbox_request::Request::Fetch(record)) => {
                let expected = nonce.take();
                let Ok((mailbox, fetch)) =
                    owner_request::<proto::Fetch>(&record, Domain::MailboxFetch, |f| {
                        fresh(expected, &f.nonce)
                    })
                else {
                    return Ok(status(Status::Denied));
                };
                if self.listed(&mailbox) {
                    return Ok(status(Status::Refused));
                }
                let limit = match fetch.limit {
                    0 => MAX_FETCH,
                    limit => limit.min(MAX_FETCH),
                };
                // `watch` is kept by the node loop, which owns the connections.
                let (envelopes, more) = self.store.fetch_envelopes(&mailbox, limit, FETCH_BYTES)?;
                Ok(MailboxResponse {
                    envelopes,
                    more,
                    ..status(Status::Ok)
                })
            }
            Some(mailbox_request::Request::Ack(record)) => {
                let expected = nonce.take();
                self.ack(&record, |ack| fresh(expected, &ack.nonce))
            }
            // ponytail: a replayed ack deletes only ids its owner already acked; ids are random,
            // so no new envelope reuses them.
            Some(mailbox_request::Request::ReplicaAck(record)) => self.ack(&record, |_| true),
            Some(mailbox_request::Request::Inventory(inventory)) => self.inventory(&inventory),
            Some(mailbox_request::Request::ReplicaPut(batch)) => self.replica_put(peer, &batch),
            // A replayed revoke only revokes the same keys again.
            Some(
                mailbox_request::Request::Revoke(record)
                | mailbox_request::Request::ReplicaRevoke(record),
            ) => {
                let (response, strikes) = self.revoke(&record)?;
                *strike = strikes;
                Ok(response)
            }
            None => Ok(status(Status::Unsupported)),
        }
    }

    fn ack(
        &self,
        record: &SignedRecord,
        check: impl Fn(&proto::Ack) -> bool,
    ) -> crate::Result<MailboxResponse> {
        let Ok((mailbox, ack)) = owner_request(record, Domain::MailboxAck, check) else {
            return Ok(status(Status::Denied));
        };
        if ack.ids.iter().any(|id| id.len() != 16) {
            return Ok(status(Status::Invalid));
        }
        self.store.ack_envelopes(&mailbox, &ack.ids)?;
        Ok(status(Status::Ok))
    }

    /// The ids of `inventory` this node lacks; refused from the repair share of the byte rate.
    fn inventory(&self, inventory: &proto::Inventory) -> crate::Result<MailboxResponse> {
        if inventory.mailbox.len() != 32
            || inventory.ids.len() > MAX_INVENTORY
            || inventory.ids.iter().any(|id| id.len() != 16)
        {
            return Ok(status(Status::Invalid));
        }
        if self.listed(&inventory.mailbox) {
            return Ok(status(Status::Refused));
        }
        if self.busy(SHED_MEDIA) {
            return Ok(status(Status::RateLimited));
        }
        let held: HashSet<Vec<u8>> = self.held_ids(&inventory.mailbox)?.into_iter().collect();
        Ok(MailboxResponse {
            missing: inventory
                .ids
                .iter()
                .filter(|id| !held.contains(*id))
                .cloned()
                .collect(),
            ..status(Status::Ok)
        })
    }

    /// One status for the batch: DENIED if any envelope is forged (a strike), else the first
    /// failure. ponytail: the sender rate limit applies per envelope, so a large gap of one
    /// sender fills over several repairs.
    fn replica_put(&self, peer: &Peer, batch: &proto::Envelopes) -> crate::Result<MailboxResponse> {
        let mut worst = status(Status::Ok);
        // ponytail: a repaired envelope gets a fresh TTL here, so replicas can re-seed an envelope a
        // watching device never acks until the mailbox cap; carry the expiry if that matters.
        for (i, record) in batch.envelopes.iter().enumerate() {
            let group = Group::Label(batch.source_groups.get(i).map_or(&[], Vec::as_slice));
            let put = self.put(
                record,
                true,
                Source::Replica,
                ENVELOPE_ACCEPT_BYTES,
                &peer.id,
                group,
            )?;
            if worst.status == i32::from(Status::Ok) || put.status == i32::from(Status::Denied) {
                worst = put;
            }
        }
        Ok(worst)
    }

    /// The ids of the first [`MAX_INVENTORY`] envelopes of a mailbox in fetch order, read without
    /// the records.
    pub fn held_ids(&self, mailbox: &[u8]) -> crate::Result<Vec<Vec<u8>>> {
        self.store.envelope_ids(mailbox, MAX_INVENTORY as u32)
    }

    /// The oldest [`MAX_INVENTORY`] envelopes of a mailbox by id, with their source groups, for
    /// repair.
    pub fn held(&self, mailbox: &[u8]) -> crate::Result<HashMap<Vec<u8>, Held>> {
        let held = self.store.held_envelopes(mailbox, MAX_INVENTORY as u32)?;
        Ok(held
            .into_iter()
            .filter_map(|(r, group)| {
                Some((
                    proto::Envelope::decode(r.payload.as_slice()).ok()?.id,
                    (r, group),
                ))
            })
            .collect())
    }

    /// Stores a locally held replica after admission, without billing the client twice. `local`
    /// is this node's peer ID and `group` the source group from [`Service::put_group`].
    pub fn store_mailbox(
        &self,
        record: &SignedRecord,
        local: &str,
        group: &[u8],
    ) -> crate::Result<MailboxResponse> {
        let group = Group::Label(group);
        self.put(
            record,
            true,
            Source::Local,
            ENVELOPE_ACCEPT_BYTES,
            local,
            group,
        )
    }

    /// The source group of a client put from `peer`: the salted hash of the mailbox and the
    /// peer's IP group, empty for an undecodable envelope.
    pub fn put_group(&self, record: &SignedRecord, peer: &Peer) -> Vec<u8> {
        proto::Envelope::decode(record.payload.as_slice())
            .map(|envelope| self.source_group(&envelope.mailbox, &peer.group))
            .unwrap_or_default()
    }

    fn source_group(&self, mailbox: &[u8], ip_group: &str) -> Vec<u8> {
        label(&[&self.salt[..], mailbox, ip_group.as_bytes()].concat()).to_vec()
    }

    /// `via` is the peer ID the envelope came from; `group` its sender's source group.
    pub(super) fn put(
        &self,
        record: &SignedRecord,
        store: bool,
        source: Source,
        max: usize,
        via: &str,
        group: Group,
    ) -> crate::Result<MailboxResponse> {
        Ok(self.put_inner(record, store, source, max, via, group)?.0)
    }

    /// [`Service::put`], and whether a DENIED answer strikes the sender.
    fn put_inner(
        &self,
        record: &SignedRecord,
        store: bool,
        source: Source,
        max: usize,
        via: &str,
        group: Group,
    ) -> crate::Result<(MailboxResponse, bool)> {
        if record.payload.len() > max {
            return Ok((mailbox_too_large(max), true));
        }
        if record.verify(Domain::Envelope).is_err() {
            return Ok((status(Status::Denied), true));
        }
        let Ok(envelope) = proto::Envelope::decode(record.payload.as_slice()) else {
            return Ok((status(Status::Invalid), true));
        };
        if envelope.id.len() != 16 || envelope.mailbox.len() != 32 {
            return Ok((status(Status::Invalid), true));
        }
        let token_key = match self.admit_token(record, &envelope, source)? {
            Ok(key) => key,
            Err((answer, strike)) => return Ok((status(answer), strike)),
        };
        self.store_put(record, &envelope, token_key, store, source, via, group)
            .map(|response| (response, true))
    }

    /// The rest of [`Service::put`] once the token is admitted.
    #[allow(clippy::too_many_arguments)]
    fn store_put(
        &self,
        record: &SignedRecord,
        envelope: &proto::Envelope,
        token_key: Option<Vec<u8>>,
        store: bool,
        source: Source,
        via: &str,
        group: Group,
    ) -> crate::Result<MailboxResponse> {
        let signal = is_signal(envelope);
        if signal && record.payload.len() > MAX_SIGNAL_BYTES {
            return Ok(mailbox_too_large(MAX_SIGNAL_BYTES));
        }
        if self.listed_key(&record.public_key) || self.listed(&envelope.mailbox) {
            return Ok(status(Status::Refused));
        }
        if source != Source::Local && !self.senders.check_limit(&hex(&record.public_key)) {
            return Ok(status(Status::RateLimited));
        }
        if !store {
            return Ok(status(Status::Ok));
        }
        let max_mb = self.config.limits.messages_max_mb;
        if self.full(
            1,
            BootstrapStore::messages_usage,
            BootstrapStore::evict_envelopes,
            max_mb,
        )? {
            return Ok(status(Status::Full));
        }
        let expires_at = chrono::Utc::now().timestamp() + self.retention() as i64;
        let group = match group {
            Group::Ip(ip_group) => self.source_group(&envelope.mailbox, ip_group),
            Group::Label(label) if label.len() <= 8 => label.to_vec(),
            Group::Label(_) => Vec::new(),
        };
        let via = label(via.as_bytes());
        let new = crate::storage::NewEnvelope {
            token_key: token_key.as_deref(),
            via: &via,
            source_group: &group,
            ..crate::storage::NewEnvelope::new(
                &envelope.mailbox,
                &envelope.id,
                record,
                signal,
                expires_at,
            )
        };
        let quota = if signal {
            self.signal_quota()
        } else {
            MAX_MAILBOX_BYTES
        };
        let tokens = self.config.limits.require_write_tokens;
        let pool = tokens.then(|| self.mailbox_pool());
        let stored = self.store.put_envelope(&new, quota, pool)?;
        if stored == Put::Stored && token_key.is_none() {
            // Pools join the store-wide share only once they are in use.
            let share = (max_mb << 20) / 100 * u64::from(self.config.limits.tokenless_share_pct);
            self.store.evict_tokenless(share, tokens)?;
        }
        Ok(status(match stored {
            Put::Full => Status::Full,
            Put::Stored | Put::Dropped => Status::Ok,
        }))
    }

    /// The envelope's write token key, `None` without a token, or the answer that refuses it and
    /// whether a DENIED one strikes.
    #[allow(clippy::type_complexity)]
    fn admit_token(
        &self,
        record: &SignedRecord,
        envelope: &proto::Envelope,
        source: Source,
    ) -> crate::Result<Result<Option<Vec<u8>>, (Status, bool)>> {
        let late = match source {
            Source::Client => 3600,
            Source::Replica | Source::Local => self.retention() as i64 + 3600,
        };
        let now = chrono::Utc::now().timestamp();
        let checked = match check_token(record, envelope, now, late) {
            Ok(Some(key)) if self.revoked(&envelope.mailbox, &key)? => Err(TokenError::Revoked),
            checked => checked,
        };
        Ok(match (checked, source) {
            (Ok(key), _) => Ok(key),
            // Replica paths never strike: an old node relays tokens unverified.
            (Err(_), Source::Replica | Source::Local) => Err((Status::Ok, false)),
            (Err(TokenError::Forged), Source::Client) => Err((Status::Denied, true)),
            (Err(TokenError::Revoked), Source::Client) => Err((Status::Denied, false)),
            (Err(TokenError::Expired), Source::Client) => Err((Status::Stale, false)),
        })
    }
}

/// Who hands an envelope to [`Service::put`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    /// A client put: billed to the sender; a write token must be current.
    Client,
    /// A replica put or repair: billed; a token may be one TTL late, and a bad one drops the
    /// envelope with `OK`.
    Replica,
    /// A local replica of an admitted client put: not billed again.
    Local,
}

/// A held envelope and its source group.
pub type Held = (SignedRecord, Vec<u8>);

/// Where the source group of an envelope comes from.
#[derive(Clone, Copy)]
pub(super) enum Group<'a> {
    /// The IP group of the client that sent it: this node accepts it.
    Ip(&'a str),
    /// The accepting node's label, carried unsigned; one longer than 8 bytes is read as empty.
    Label(&'a [u8]),
}

/// An 8-byte sender label: the head of the SHA-256 of `data`.
fn label(data: &[u8]) -> [u8; 8] {
    dyapp_identity::sha256(data)[..8]
        .try_into()
        .unwrap_or_default()
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum TokenError {
    /// Bad token signature, a token for another mailbox, or an envelope not signed by its key.
    Forged,
    /// Outside the token's day, with one hour of skew, plus `late` seconds after it.
    Expired,
    /// The mailbox revoked the token key.
    Revoked,
}

/// Checks the envelope's write token at `now`; returns its key, `None` without a token.
pub(super) fn check_token(
    record: &SignedRecord,
    envelope: &proto::Envelope,
    now: i64,
    late: i64,
) -> Result<Option<Vec<u8>>, TokenError> {
    let Some(token) = &envelope.write_token else {
        return Ok(None);
    };
    let Ok((mailbox, payload)) =
        owner_request::<proto::WriteToken>(token, Domain::WriteToken, |_| true)
    else {
        return Err(TokenError::Forged);
    };
    if mailbox[..] != envelope.mailbox
        || payload.mailbox != envelope.mailbox
        || payload.token_key != record.public_key
    {
        return Err(TokenError::Forged);
    }
    let start = i64::from(payload.epoch) * 86_400;
    if now < start - 3600 || now >= start + 86_400 + late {
        return Err(TokenError::Expired);
    }
    Ok(Some(payload.token_key))
}

/// Whether `got` is the nonce issued on this connection, compared in constant time.
fn fresh(expected: Option<[u8; 32]>, got: &[u8]) -> bool {
    expected.is_some_and(|expected| subtle::ConstantTimeEq::ct_eq(got, &expected[..]).into())
}

/// A SIGNAL envelope; any other class, unknown values included, is a message.
pub(crate) fn is_signal(envelope: &proto::Envelope) -> bool {
    envelope.class == proto::EnvelopeClass::Signal as i32
}

fn mailbox_too_large(max: usize) -> MailboxResponse {
    MailboxResponse {
        max_bytes: max as u64,
        ..status(Status::TooLarge)
    }
}
