//! Remote replies, rather than request dispatch, settle profile and mailbox handovers.
use super::*;

struct Handover {
    store: Store,
    key: Vec<u8>,
    source: Service,
    swarm: Swarm<Behaviour>,
    replicas: Replicas,
    job: u64,
    remote: PeerId,
    now: Instant,
}

impl Handover {
    fn begin(store: Store) -> Self {
        let (source, mut swarm) = node();
        let key = match store {
            Store::Mailbox => {
                hold_mailbox(&source, 41);
                vec![41; 32]
            }
            Store::Profile => {
                let owner = dyapp_identity::Identity::from_secret(&[42; 32]);
                let record = dyapp_profile::Profile {
                    version: 2,
                    ..Default::default()
                }
                .sign(&owner);
                assert_eq!(
                    source.store_profile(&record).unwrap().status,
                    i32::from(Status::Ok)
                );
                dyapp_profile::verify(&record).unwrap().peer_id.into_bytes()
            }
            Store::Media => unreachable!(),
        };
        let mut replicas = Replicas::default();
        let now = ready(&mut replicas);
        replicas.scan(&mut swarm, &source, now);
        replicas.republish(&mut swarm, &source, now);
        assert_eq!(replicas.republish.len(), 1);
        let job = *replicas.republish.queries.values().next().unwrap();
        finish_lookups(&mut replicas, true);
        let remote = remote(&mut swarm);
        Self {
            store,
            key,
            source,
            swarm,
            replicas,
            job,
            remote,
            now,
        }
    }

    fn inventory(&mut self) -> OutboundRequestId {
        let id = match self.store {
            Store::Mailbox => {
                let request = MailboxRequest {
                    request: Some(mailbox_request::Request::Inventory(proto::Inventory {
                        mailbox: self.key.clone(),
                        ids: self.source.held_ids(&self.key).unwrap(),
                    })),
                };
                let id = self
                    .swarm
                    .behaviour_mut()
                    .mailbox
                    .send_request(&self.remote, request);
                self.replicas.inventories.insert(id, self.key.clone());
                id
            }
            Store::Profile => {
                let peer = std::str::from_utf8(&self.key).unwrap();
                let record = self.source.store.get_profile(peer).unwrap().unwrap();
                self.replicas
                    .profile_inventory(&mut self.swarm, &self.source, self.remote, Arc::new(record))
                    .unwrap()
                    .unwrap()
            }
            Store::Media => unreachable!(),
        };
        self.replicas.republish.request(Some(self.job), id);
        id
    }

    fn answer(&mut self, id: OutboundRequestId, status: Status) {
        match self.store {
            Store::Mailbox => {
                let missing = if status == Status::Ok {
                    self.source.held_ids(&self.key).unwrap()
                } else {
                    Vec::new()
                };
                self.replicas.mailbox_response(
                    &mut self.swarm,
                    &self.source,
                    self.remote,
                    id,
                    proto::MailboxResponse {
                        status: status.into(),
                        missing,
                        ..Default::default()
                    },
                );
            }
            Store::Profile => self.replicas.profile_response(
                &mut self.swarm,
                &self.source,
                self.remote,
                id,
                proto::ProfileResponse {
                    status: status.into(),
                    ..Default::default()
                },
            ),
            Store::Media => unreachable!(),
        }
    }

    fn answer_gap(&mut self, id: OutboundRequestId) {
        let status = match self.store {
            Store::Mailbox => Status::Ok,
            Store::Profile => Status::NotFound,
            Store::Media => unreachable!(),
        };
        self.answer(id, status);
    }

    fn fail(&mut self, id: OutboundRequestId, unsupported: bool) {
        let error = if unsupported {
            OutboundFailure::UnsupportedProtocols
        } else {
            OutboundFailure::Timeout
        };
        match self.store {
            Store::Mailbox => self.replicas.mailbox_failure(self.remote, id, &error),
            Store::Profile => self.replicas.profile_failure(self.remote, id, &error),
            Store::Media => unreachable!(),
        }
    }

    fn writes(&self) -> Vec<OutboundRequestId> {
        self.replicas.republish.requests.keys().copied().collect()
    }

    fn pending(&self, id: OutboundRequestId) {
        assert_eq!(self.replicas.republish.len(), 1);
        assert_eq!(self.replicas.republish.requests.get(&id), Some(&self.job));
    }

    fn dirty(&mut self) -> bool {
        self.replicas
            .republish(&mut self.swarm, &self.source, self.now);
        assert_eq!(self.replicas.republish.len(), 0);
        self.replicas.scan(&mut self.swarm, &self.source, self.now);
        self.replicas.republish.scanner.pop(self.store).is_some()
    }
}

fn faulty_replies_keep_handover_dirty(store: Store) {
    for status in [Status::Denied, Status::Full, Status::RateLimited] {
        let mut handover = Handover::begin(store);
        let inventory = handover.inventory();
        handover.answer(inventory, status);
        assert!(handover.dirty(), "{store:?} inventory {status:?}");

        let mut handover = Handover::begin(store);
        let inventory = handover.inventory();
        handover.answer_gap(inventory);
        let writes = handover.writes();
        assert!(!writes.is_empty(), "{store:?} sent no replica write");
        for write in writes {
            handover.answer(write, status);
        }
        assert!(handover.dirty(), "{store:?} write {status:?}");
    }
    for unsupported in [false, true] {
        let mut handover = Handover::begin(store);
        let inventory = handover.inventory();
        handover.fail(inventory, unsupported);
        assert!(handover.dirty(), "{store:?} inventory failure");

        let mut handover = Handover::begin(store);
        let inventory = handover.inventory();
        handover.answer_gap(inventory);
        for write in handover.writes() {
            handover.fail(write, unsupported);
        }
        assert!(handover.dirty(), "{store:?} write failure");
    }
}

fn successful_handover(store: Store) {
    let mut handover = Handover::begin(store);
    let inventory = handover.inventory();
    handover.answer_gap(inventory);
    for write in handover.writes() {
        handover.answer(write, Status::Ok);
    }
    assert!(!handover.dirty(), "{store:?} success stayed dirty");
}

#[test]
fn faulty_profile_replies_keep_handover_dirty() {
    faulty_replies_keep_handover_dirty(Store::Profile);
}

#[test]
fn faulty_mailbox_replies_keep_handover_dirty() {
    faulty_replies_keep_handover_dirty(Store::Mailbox);
}

#[test]
fn successful_remote_replies_complete_profile_and_mailbox_handovers() {
    successful_handover(Store::Profile);
    successful_handover(Store::Mailbox);
}

#[test]
fn unrelated_request_ids_leave_profile_and_mailbox_handovers_pending() {
    for store in [Store::Profile, Store::Mailbox] {
        let mut handover = Handover::begin(store);
        let inventory = handover.inventory();
        let unrelated = match store {
            Store::Profile => handover
                .swarm
                .behaviour_mut()
                .profile
                .send_request(&handover.remote, proto::ProfileRequest::default()),
            Store::Mailbox => handover
                .swarm
                .behaviour_mut()
                .mailbox
                .send_request(&handover.remote, MailboxRequest::default()),
            Store::Media => unreachable!(),
        };
        handover.answer(unrelated, Status::Ok);
        handover.pending(inventory);
    }
}
