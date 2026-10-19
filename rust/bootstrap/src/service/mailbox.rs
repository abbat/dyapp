//! The mailbox requests: put, fetch, ack and node-to-node repair.

use super::*;

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
        let response = self.mailbox_request(nonce, request, store)?;
        if response.status == i32::from(Status::Denied) {
            self.strike(peer);
        }
        Ok(response)
    }

    fn mailbox_request(
        &self,
        nonce: &mut Option<[u8; 32]>,
        request: MailboxRequest,
        store: bool,
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
                self.put(&record, store, true, MAX_ENVELOPE_BYTES)
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
            Some(mailbox_request::Request::ReplicaPut(batch)) => self.replica_put(&batch),
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
    fn replica_put(&self, batch: &proto::Envelopes) -> crate::Result<MailboxResponse> {
        let mut worst = status(Status::Ok);
        // ponytail: a repaired envelope gets a fresh TTL here, so replicas can re-seed an envelope a
        // watching device never acks until the mailbox cap; carry the expiry if that matters.
        for record in &batch.envelopes {
            let put = self.put(record, true, true, ENVELOPE_ACCEPT_BYTES)?;
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

    /// The oldest [`MAX_INVENTORY`] envelopes of a mailbox by id, for repair.
    pub fn held(&self, mailbox: &[u8]) -> crate::Result<HashMap<Vec<u8>, SignedRecord>> {
        let (records, _) = self
            .store
            .fetch_envelopes(mailbox, MAX_INVENTORY as u32, u64::MAX)?;
        Ok(records
            .into_iter()
            .filter_map(|r| Some((proto::Envelope::decode(r.payload.as_slice()).ok()?.id, r)))
            .collect())
    }

    /// Stores a locally held replica after admission, without billing the client twice.
    pub fn store_mailbox(&self, record: &SignedRecord) -> crate::Result<MailboxResponse> {
        self.put(record, true, false, ENVELOPE_ACCEPT_BYTES)
    }

    pub(super) fn put(
        &self,
        record: &SignedRecord,
        store: bool,
        bill_sender: bool,
        max: usize,
    ) -> crate::Result<MailboxResponse> {
        if record.payload.len() > max {
            return Ok(mailbox_too_large(max));
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
        let signal = is_signal(&envelope);
        if signal && record.payload.len() > MAX_SIGNAL_BYTES {
            return Ok(mailbox_too_large(MAX_SIGNAL_BYTES));
        }
        if self.listed_key(&record.public_key) || self.listed(&envelope.mailbox) {
            return Ok(status(Status::Refused));
        }
        if bill_sender && !self.senders.check_limit(&hex(&record.public_key)) {
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
        let stored = self.store.put_envelope(
            &envelope.mailbox,
            &envelope.id,
            record,
            signal,
            expires_at,
            if signal {
                self.signal_quota()
            } else {
                MAX_MAILBOX_BYTES
            },
        )?;
        Ok(status(if stored { Status::Ok } else { Status::Full }))
    }
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
