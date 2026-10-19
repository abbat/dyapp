//! The search role's requests, gossip and catch-up.

use super::*;

/// Search get replies are cut here, like the profile record limit.
const MAX_SEARCH_REPLY: usize = 1 << 20;
/// Keys in one search get.
const MAX_SEARCH_GET: usize = 50;
/// Keys in one inventory reply.
const MAX_SEARCH_INVENTORY: u32 = 10_000;

pub(super) fn search_reply(status: Status) -> SearchResponse {
    SearchResponse {
        status: status.into(),
        ..SearchResponse::default()
    }
}

/// The key hash of a publish or heartbeat, which picks its gossip topic.
pub fn search_key(request: &SearchRequest) -> Option<[u8; 32]> {
    match &request.request {
        Some(search_request::Request::Publish(r) | search_request::Request::Heartbeat(r)) => {
            let key: [u8; 32] = r.public_key.as_slice().try_into().ok()?;
            Some(dyapp_identity::key_hash(&key))
        }
        _ => None,
    }
}

/// What applying a publish or heartbeat did, before it becomes a reply or a gossip verdict.
pub(super) enum Applied {
    Done,
    /// Valid, but of a topic this node does not keep.
    Skipped,
    /// A bad signature: a strike for the sender.
    Bad,
    Status(Status),
    TooLarge(usize),
    RequiredPow(u32),
    Clock(u64),
    Limited(u64),
    /// The proof of work is not cached: it needs an Argon2 check first.
    Pow(PowCheck),
}

/// A search request after admission: a ready reply, or read work the node runs in
/// `spawn_blocking` so a slow query never holds the swarm loop.
pub enum Search {
    /// The reply, ready now.
    Reply(SearchResponse),
    /// A query to run against the index.
    Query(proto::SearchQuery),
    /// Records of these keys to read from the index.
    Get(Vec<[u8; 32]>),
    /// A publish or heartbeat whose proof of work is not cached: the node checks it off the
    /// loop, then calls [`Service::search_pow_done`].
    Pow(PowCheck, SearchRequest),
}

/// The refusal of a heartbeat, or `None` when it may be counted.
fn beaten(beat: crate::search::Beat) -> Option<Applied> {
    match beat {
        crate::search::Beat::Ok => None,
        crate::search::Beat::NotFound => Some(Applied::Status(Status::NotFound)),
        crate::search::Beat::Limited { retry_after } => Some(Applied::Limited(retry_after)),
    }
}

/// Runs `query`, scanning at most `max_scan_rows` rows.
pub fn run_query(
    index: &SearchIndex,
    query: &proto::SearchQuery,
    max_scan_rows: u32,
) -> crate::Result<SearchResponse> {
    Ok(
        match index.query(&query.conditions, query.limit, max_scan_rows)? {
            Err(()) => search_reply(Status::Invalid),
            Ok(found) => SearchResponse {
                hits: found
                    .hits
                    .into_iter()
                    .map(|(key, version)| proto::SearchHit {
                        key: key.to_vec(),
                        version,
                    })
                    .collect(),
                applied: found.applied,
                partial: found.partial,
                index_version: crate::search::INDEX_VERSION,
                ..search_reply(Status::Ok)
            },
        },
    )
}

/// The indexed records of `keys`, bounded by the reply size.
pub fn run_get(index: &SearchIndex, keys: &[[u8; 32]]) -> crate::Result<SearchResponse> {
    Ok(SearchResponse {
        records: index.records(keys, MAX_SEARCH_REPLY)?,
        ..search_reply(Status::Ok)
    })
}

impl Service {
    /// Errors as in [`Service::profile`].
    pub fn search(&self, peer: &Peer, request: SearchRequest) -> crate::Result<Search> {
        let reply = |status| Ok(Search::Reply(search_reply(status)));
        if self.search.is_none() {
            return reply(Status::Unsupported);
        }
        // A get is also charged by its reply size in `charge_get`; search nodes only there.
        let units = match &request.request {
            Some(search_request::Request::Publish(_) | search_request::Request::Heartbeat(_)) => 5,
            Some(search_request::Request::Get(_)) if self.is_search_peer(peer) => 0,
            _ => 1,
        };
        if self.shed(request.encoded_len(), SHED_MEDIA)
            || (units > 0 && !self.admit_units(peer, units))
        {
            return reply(Status::RateLimited);
        }
        if self.refused(peer) {
            return reply(Status::Refused);
        }
        let pow = |applied, request| match applied {
            Applied::Pow(check) => Search::Pow(
                check,
                SearchRequest {
                    request: Some(request),
                },
            ),
            applied => Search::Reply(self.applied(peer, applied)),
        };
        Ok(match request.request {
            Some(search_request::Request::Publish(record)) => pow(
                self.publish_search(&record, true, self.config.search.max_profile_bytes)?,
                search_request::Request::Publish(record),
            ),
            Some(search_request::Request::Heartbeat(record)) => pow(
                self.beat_search(&record)?,
                search_request::Request::Heartbeat(record),
            ),
            Some(search_request::Request::Query(query)) => {
                match self.puzzled(peer, &query.puzzle) {
                    Some(reply) => reply,
                    None => Search::Query(query),
                }
            }
            Some(search_request::Request::Get(get)) => {
                if let Some(reply) = self.puzzled(peer, &get.puzzle) {
                    return Ok(reply);
                }
                let keys: Option<Vec<[u8; 32]>> = (get.keys.len() <= MAX_SEARCH_GET)
                    .then(|| {
                        get.keys
                            .iter()
                            .map(|k| k.as_slice().try_into().ok())
                            .collect()
                    })
                    .flatten();
                let Some(keys) = keys else {
                    return reply(Status::Invalid);
                };
                Search::Get(keys.into_iter().filter(|k| !self.listed(k)).collect())
            }
            None => Search::Reply(search_reply(Status::Unsupported)),
        })
    }

    /// Charges a built get reply: a unit per started 64 KiB for clients, `search_sync` bytes for
    /// search nodes. `false` means the node replies `RATE_LIMITED` instead.
    pub fn charge_get(&self, peer: &Peer, bytes: usize) -> bool {
        if self.is_search_peer(peer) {
            return self.search_sync.try_add(bytes as u64, 100);
        }
        let units = u32::try_from(bytes.div_ceil(64 << 10)).unwrap_or(u32::MAX);
        units == 0 || self.admit_units(peer, units)
    }

    /// Marks `peer_id` as a connected search node or forgets it.
    pub fn search_peer(&self, peer_id: &str, on: bool) {
        let Ok(mut peers) = self.search_peers.lock() else {
            return;
        };
        if on {
            peers.insert(peer_id.to_owned());
        } else {
            peers.remove(peer_id);
        }
    }

    /// The `PUZZLE` reply to a client's query or get without a valid solution while the node
    /// asks for one; search nodes skip it. The node consumes the solution on admission.
    pub(super) fn puzzled(&self, peer: &Peer, puzzle: &Option<proto::Puzzle>) -> Option<Search> {
        let (challenge, bits) = self.puzzle.current();
        if bits == 0
            || self.is_search_peer(peer)
            || puzzle
                .as_ref()
                .is_some_and(|p| self.puzzle.check(&peer.id, p))
        {
            return None;
        }
        Some(Search::Reply(SearchResponse {
            puzzle_challenge: challenge.to_vec(),
            puzzle_bits: bits,
            ..search_reply(Status::Puzzle)
        }))
    }

    /// Whether `peer` is a connected search node.
    pub fn is_search_peer(&self, peer: &Peer) -> bool {
        self.search_peers
            .lock()
            .is_ok_and(|peers| peers.contains(&peer.id))
    }

    pub(super) fn applied(&self, peer: &Peer, applied: Applied) -> SearchResponse {
        match applied {
            Applied::Done | Applied::Skipped => search_reply(Status::Ok),
            Applied::Bad => {
                self.strike(peer);
                search_reply(Status::Denied)
            }
            Applied::Status(status) => search_reply(status),
            Applied::TooLarge(max) => SearchResponse {
                max_bytes: max as u64,
                ..search_reply(Status::TooLarge)
            },
            Applied::RequiredPow(required_pow_bits) => SearchResponse {
                required_pow_bits,
                ..search_reply(Status::Invalid)
            },
            Applied::Clock(server_time) => SearchResponse {
                server_time,
                ..search_reply(Status::Invalid)
            },
            Applied::Limited(retry_after) => SearchResponse {
                retry_after,
                ..search_reply(Status::RateLimited)
            },
            // The cache was cleared between the check and this run: the client retries.
            Applied::Pow(_) => SearchResponse {
                retry_after: 1,
                ..search_reply(Status::RateLimited)
            },
        }
    }

    /// Finishes a publish or heartbeat after its off-loop proof-of-work check; a forged proof
    /// is a strike.
    pub fn search_pow_done(
        &self,
        peer: &Peer,
        request: &SearchRequest,
        ok: bool,
    ) -> crate::Result<SearchResponse> {
        let applied = match &request.request {
            _ if !ok => Applied::Bad,
            Some(search_request::Request::Publish(record)) => {
                self.publish_search(record, true, self.config.search.max_profile_bytes)?
            }
            Some(search_request::Request::Heartbeat(record)) => self.beat_search(record)?,
            _ => Applied::Status(Status::Unsupported),
        };
        Ok(self.applied(peer, applied))
    }

    /// `limit = false` (catch-up) skips only the 10-minute update limit.
    pub(super) fn publish_search(
        &self,
        record: &SignedRecord,
        limit: bool,
        max: usize,
    ) -> crate::Result<Applied> {
        let Some(index) = &self.search else {
            return Ok(Applied::Status(Status::Unsupported));
        };
        if record.encoded_len() > max {
            return Ok(Applied::TooLarge(max));
        }
        let verified = match dyapp_profile::verify(record) {
            Ok(verified) => verified,
            Err(dyapp_profile::Error::Signature(_)) => return Ok(Applied::Bad),
            Err(_) => return Ok(Applied::Status(Status::Invalid)),
        };
        // A verified record has a valid 32-byte key.
        let signer: [u8; 32] = record.public_key.as_slice().try_into().unwrap_or_default();
        let key = dyapp_identity::key_hash(&signer);
        if self.listed(&key) {
            return Ok(Applied::Status(Status::Refused));
        }
        if !self.config.search.keeps(crate::search::topic(&key)) {
            return Ok(Applied::Skipped);
        }
        let profile = &verified.profile;
        if profile.pow_bits < self.config.search.min_profile_pow_bits {
            return Ok(Applied::RequiredPow(
                self.config.search.min_profile_pow_bits,
            ));
        }
        let check = PowCheck {
            key: signer,
            nonce: profile.pow_nonce,
            bits: profile.pow_bits,
            day: None,
        };
        if !self.pow.confirmed(&check) {
            return Ok(Applied::Pow(check));
        }
        let now = chrono::Utc::now().timestamp();
        Ok(
            match index.put(&key, &verified.profile, record, now, limit)? {
                crate::search::Put::Stored => Applied::Done,
                crate::search::Put::Stale => Applied::Status(Status::Stale),
                crate::search::Put::Limited { retry_after } => Applied::Limited(retry_after),
            },
        )
    }

    pub(super) fn beat_search(&self, record: &SignedRecord) -> crate::Result<Applied> {
        self.beat_search_at(record, chrono::Utc::now().timestamp())
    }

    /// Cheap checks first: the signature, the clock, the stored version and day limit, then
    /// the daily Argon2 proof, so a replayed or surplus beat never costs a check.
    pub(super) fn beat_search_at(&self, record: &SignedRecord, now: i64) -> crate::Result<Applied> {
        let Some(index) = &self.search else {
            return Ok(Applied::Status(Status::Unsupported));
        };
        let Ok((key, beat)) =
            owner_request::<proto::Heartbeat>(record, Domain::Heartbeat, |_| true)
        else {
            return Ok(Applied::Bad);
        };
        if self.listed(&key) {
            return Ok(Applied::Status(Status::Refused));
        }
        if beat.time.abs_diff(now.unsigned_abs()) > CLOCK_SKEW {
            return Ok(Applied::Clock(now.unsigned_abs()));
        }
        if !self.config.search.keeps(crate::search::topic(&key)) {
            return Ok(Applied::Skipped);
        }
        let min = self.config.search.min_profile_pow_bits;
        if min > 0 && index.pow_bits(&key)?.is_some_and(|bits| bits < min) {
            return Ok(Applied::RequiredPow(min));
        }
        let min = self.config.search.min_heartbeat_pow_bits;
        if min > 0 {
            if let Some(refused) = beaten(index.can_beat(&key, beat.version, now)?) {
                return Ok(refused);
            }
            if beat.pow_bits < min || i64::from(beat.pow_day) != now.div_euclid(86_400) {
                return Ok(Applied::Status(Status::Invalid));
            }
            let check = PowCheck {
                key: record.public_key.as_slice().try_into().unwrap_or_default(),
                nonce: beat.pow_nonce,
                bits: beat.pow_bits,
                day: Some(beat.pow_day),
            };
            if !self.pow.confirmed(&check) {
                return Ok(Applied::Pow(check));
            }
        }
        Ok(beaten(index.heartbeat(&key, beat.version, now)?).unwrap_or(Applied::Done))
    }

    /// Applies a gossiped publish or heartbeat. On `Reject` the node strikes the peer that
    /// propagated it.
    pub fn search_gossip(&self, data: &[u8]) -> Verdict {
        let Ok(request) = SearchRequest::decode(data) else {
            return Verdict::Reject;
        };
        let applied = match &request.request {
            Some(search_request::Request::Publish(record)) => {
                self.publish_search(record, true, SEARCH_ACCEPT_BYTES)
            }
            Some(search_request::Request::Heartbeat(record)) => self.beat_search(record),
            _ => return Verdict::Reject,
        };
        match applied {
            Ok(Applied::Done) => Verdict::Accept,
            Ok(Applied::Bad) => Verdict::Reject,
            Ok(Applied::Pow(check)) => Verdict::Pow(check),
            Ok(_) => Verdict::Ignore,
            Err(error) => {
                tracing::error!(%error, "search gossip not stored");
                Verdict::Ignore
            }
        }
    }

    /// Stores a record fetched in catch-up, without the update limit.
    pub fn search_apply(&self, record: &SignedRecord) -> crate::Result<Caught> {
        Ok(
            match self.publish_search(record, false, SEARCH_ACCEPT_BYTES)? {
                Applied::Done => Caught::Stored,
                Applied::Pow(check) => Caught::Pow(check),
                _ => Caught::Dropped,
            },
        )
    }

    /// `/dyapp/inventory`: the key hashes and versions of one topic, for catch-up.
    pub fn search_inventory(
        &self,
        peer: &Peer,
        request: InventoryRequest,
    ) -> crate::Result<InventoryResponse> {
        let reply = |status: Status| InventoryResponse {
            status: status.into(),
            have: vec![],
        };
        let Some(index) = &self.search else {
            return Ok(reply(Status::Unsupported));
        };
        let sync = self.is_search_peer(peer);
        if self.shed(request.encoded_len(), SHED_MEDIA) || (!sync && !self.admit_units(peer, 10)) {
            return Ok(reply(Status::RateLimited));
        }
        if self.refused(peer) {
            return Ok(reply(Status::Refused));
        }
        if request.topic >= crate::search::SEARCH_TOPICS || request.after.len() > 32 {
            return Ok(reply(Status::Invalid));
        }
        let have = index.inventory(request.topic, &request.after, &[], MAX_SEARCH_INVENTORY)?;
        let response = InventoryResponse {
            status: Status::Ok.into(),
            have: have
                .into_iter()
                .map(|(key, version)| proto::SearchHit {
                    key: key.to_vec(),
                    version,
                })
                .collect(),
        };
        if sync && !self.search_sync.try_add(response.encoded_len() as u64, 100) {
            return Ok(reply(Status::RateLimited));
        }
        Ok(response)
    }
}
