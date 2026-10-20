//! Mailbox delivery: repair sends, follow-ups of mailbox requests, watcher pushes and lookups.

use super::*;

/// Envelope bytes selected by one repair; a larger gap fills over the next repairs.
pub(super) const REPAIR_BYTES: usize = 1024 * 1024;

/// Sends the mailbox's unexpired revokes, one unit each, then wanted held messages in immediately
/// admitted batches of at most 50 units. Signals are never repaired: a signal evicted at its quota
/// would come back from another replica and evict the next one.
pub(super) fn send_envelopes(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    cap: &mut ReplicaCap,
    to: PeerId,
    mailbox: &[u8],
    wanted: impl Fn(&Vec<u8>) -> bool,
) {
    let revokes = match service.revokes(mailbox) {
        Ok(revokes) => revokes,
        Err(error) => return tracing::error!(%error, "mailbox repair failed"),
    };
    for record in revokes {
        if !cap.allow(&to, 1) {
            continue;
        }
        let request = MailboxRequest {
            request: Some(mailbox_request::Request::ReplicaRevoke(record)),
        };
        service.traffic.add(request.encoded_len() as u64);
        swarm.behaviour_mut().mailbox.send_request(&to, request);
    }
    let held = match service.held(mailbox) {
        Ok(held) => held,
        Err(error) => return tracing::error!(%error, "mailbox repair failed"),
    };
    let mut bytes = 0;
    let now = chrono::Utc::now().timestamp();
    let envelopes: Vec<crate::service::Held> = held
        .into_iter()
        .filter(|(id, (record, _, _))| {
            wanted(id)
                && proto::Envelope::decode(record.payload.as_slice())
                    .is_ok_and(|e| !crate::service::is_signal(&e))
        })
        .map(|(_, held)| held)
        .take_while(|(record, _, _)| {
            bytes += record.encoded_len();
            bytes <= REPAIR_BYTES
        })
        .collect();
    for batch in envelopes.chunks(usize::try_from(REPLICA_UNITS_PER_SECOND).unwrap()) {
        if !cap.allow(&to, u32::try_from(batch.len()).unwrap()) {
            continue;
        }
        let request = MailboxRequest {
            request: Some(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                envelopes: batch.iter().map(|(record, _, _)| record.clone()).collect(),
                source_groups: batch.iter().map(|(_, group, _)| group.clone()).collect(),
                remaining_seconds: batch
                    .iter()
                    .map(|(_, _, expires_at)| {
                        u32::try_from((expires_at - now).max(1)).unwrap_or(u32::MAX)
                    })
                    .collect(),
            })),
        };
        service.traffic.add(request.encoded_len() as u64);
        swarm.behaviour_mut().mailbox.send_request(&to, request);
    }
}

/// What follows a successful mailbox request: a put is pushed to the mailbox's watchers; a
/// watching fetch is remembered and starts a repair; a device's ack or revoke is forwarded to the
/// nodes closest to the replica keys (their replies are ignored: the envelopes expire anyway); an
/// inventory's sender gets the envelopes it lacks if it is near a replica key.
/// ponytail: acks and inventories go to the closest node of each replica key only, where clients
/// write; a replica written elsewhere because that node was full is not repaired and keeps an
/// acked envelope until it expires.
pub(super) fn after_mailbox(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    watchers: &mut Watchers,
    replicas: &mut Replicas,
    (peer, connection): (PeerId, ConnectionId),
    request: Option<mailbox_request::Request>,
) {
    let owner = |record: &SignedRecord| {
        <[u8; 32]>::try_from(record.public_key.as_slice())
            .map(|key| dyapp_identity::key_hash(&key).to_vec())
    };
    let mut lookup = |swarm: &mut Swarm<Behaviour>, mailbox: &[u8], then: &dyn Fn() -> Lookup| {
        for i in 0..REPLICAS {
            let key = replica_key(mailbox, i);
            let query = swarm.behaviour_mut().kad.get_closest_peers(key);
            replicas.lookups.insert(query, then());
        }
    };
    let repaired = &mut replicas.repaired;
    match request {
        Some(mailbox_request::Request::Put(record)) => push(swarm, watchers, vec![record]),
        // ponytail: a batch with one failed envelope is not pushed; the device fetches the rest.
        Some(mailbox_request::Request::ReplicaPut(batch)) => push(swarm, watchers, batch.envelopes),
        Some(mailbox_request::Request::Fetch(record)) => {
            let watch = proto::Fetch::decode(record.payload.as_slice()).is_ok_and(|f| f.watch);
            let (true, Ok(mailbox)) = (watch, owner(&record)) else {
                return;
            };
            let list = watchers.entry(mailbox.clone()).or_default();
            if !list.contains(&(peer, connection)) {
                list.push((peer, connection));
            }
            if service.traffic.second() < SHED_MEDIA && repaired.insert(mailbox.clone()) {
                lookup(swarm, &mailbox, &|| Lookup::Repair(mailbox.clone()));
            }
        }
        Some(mailbox_request::Request::Ack(record)) => {
            let Ok(mailbox) = owner(&record) else { return };
            let request = || mailbox_request::Request::ReplicaAck(record.clone());
            lookup(swarm, &mailbox, &|| Lookup::Forward(request()));
        }
        Some(mailbox_request::Request::Revoke(record)) => {
            let Ok(mailbox) = owner(&record) else { return };
            let request = || mailbox_request::Request::ReplicaRevoke(record.clone());
            lookup(swarm, &mailbox, &|| Lookup::Forward(request()));
        }
        Some(mailbox_request::Request::Inventory(inventory)) => {
            // An arbitrary peer may send an inventory; only one this node's routing table
            // places near a replica key gets envelopes back.
            let near = (0..REPLICAS).any(|i| {
                let key = kad::KBucketKey::new(replica_key(&inventory.mailbox, i));
                let kad = &mut swarm.behaviour_mut().kad;
                let mut closest = kad.get_closest_local_peers(&key).take(REPLICAS.into());
                closest.any(|p| *p.preimage() == peer)
            });
            if near && replicas.trust.trusted(&peer) {
                let listed: HashSet<Vec<u8>> = inventory.ids.into_iter().collect();
                send_envelopes(
                    swarm,
                    service,
                    &mut replicas.cap,
                    peer,
                    &inventory.mailbox,
                    |id| !listed.contains(id),
                );
            }
        }
        _ => {}
    }
}

/// Pushes stored envelopes to the watchers of their mailboxes.
pub(super) fn push(
    swarm: &mut Swarm<Behaviour>,
    watchers: &Watchers,
    envelopes: Vec<SignedRecord>,
) {
    for record in envelopes {
        let Ok(envelope) = proto::Envelope::decode(record.payload.as_slice()) else {
            continue;
        };
        for (watcher, _) in watchers.get(&envelope.mailbox).into_iter().flatten() {
            let push = proto::MailboxPush {
                envelopes: vec![record.clone()],
            };
            swarm.behaviour_mut().push.send_request(watcher, push);
        }
    }
}

/// A finished replica-key lookup: forwards the ack, or sends the inventory unless this node is
/// itself the closest to the key.
pub(super) fn after_lookup(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    replicas: &mut Replicas,
    lookup: Lookup,
    key: Vec<u8>,
    node: kad::PeerInfo,
) {
    let (request, repair) = match lookup {
        Lookup::MediaRepair(_, _, _)
        | Lookup::Put(_)
        | Lookup::ShardPut(_, _)
        | Lookup::ShardGet(_, _)
        | Lookup::ProfileRepair(_) => unreachable!("lookups use all returned peers"),
        Lookup::Forward(request) => (request, None),
        Lookup::Repair(mailbox) => {
            let target = kad::KBucketKey::new(key);
            let me = kad::KBucketKey::from(*swarm.local_peer_id());
            if me.distance(&target) < kad::KBucketKey::from(node.peer_id).distance(&target) {
                return;
            }
            let ids = match service.held_ids(&mailbox) {
                Ok(ids) => ids,
                Err(error) => return tracing::error!(%error, "mailbox repair failed"),
            };
            let inventory = proto::Inventory {
                mailbox: mailbox.clone(),
                ids,
            };
            (
                mailbox_request::Request::Inventory(inventory),
                Some(mailbox),
            )
        }
    };
    for address in node.addrs {
        swarm.add_peer_address(node.peer_id, address);
    }
    let request = MailboxRequest {
        request: Some(request),
    };
    service.traffic.add(request.encoded_len() as u64);
    let id = swarm
        .behaviour_mut()
        .mailbox
        .send_request(&node.peer_id, request);
    if let Some(mailbox) = repair {
        replicas.inventories.insert(id, mailbox);
    }
}
