//! Replies and failures of the requests a node sends to replica holders.

use super::*;

impl Replicas {
    /// A replica lookup finished: hand its records to the closest node with proof of work.
    pub(super) fn closest(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        watchers: &Watchers,
        id: kad::QueryId,
        result: kad::GetClosestPeersResult,
    ) {
        let lookup = self.lookups.remove(&id);
        let job = self.republish.query_done(id, result.is_ok());
        if result.is_err() {
            if let Some(Lookup::MediaRepair(repair, blob, index)) = &lookup {
                if let Some((r, _)) = self.media_repairs.get_mut(repair) {
                    r.holder(*blob, *index, None);
                }
                self.finish_puts(swarm, service, Instant::now());
            }
        }
        if let (Some(lookup), Ok(ok)) = (lookup, result) {
            // A peer without the node-ID proof of work never holds a replica.
            let bits = dyapp_p2p_net::id_pow_bits();
            if matches!(
                lookup,
                Lookup::MediaRepair(_, _, _)
                    | Lookup::Put(_)
                    | Lookup::ShardPut(_, _)
                    | Lookup::ShardGet(_, _)
                    | Lookup::ProfileRepair(_)
            ) {
                let nodes: Vec<_> = ok
                    .peers
                    .into_iter()
                    .filter(|node| dyapp_p2p_net::id_has_pow(&node.peer_id, bits))
                    .collect();
                let to = holder(*swarm.local_peer_id(), &ok.key, &nodes);
                for node in nodes {
                    for address in node.addrs {
                        swarm.add_peer_address(node.peer_id, address);
                    }
                }
                match lookup {
                    Lookup::MediaRepair(repair, blob, index) => {
                        let trusted = to == *swarm.local_peer_id() || self.trust.trusted(&to);
                        if let Some((r, _)) = self.media_repairs.get_mut(&repair) {
                            r.holder(blob, index, trusted.then_some(to));
                        }
                        self.finish_puts(swarm, service, Instant::now());
                    }
                    Lookup::Put(put) => {
                        self.put_holder(swarm, service, watchers, put, to);
                        self.finish_puts(swarm, service, Instant::now());
                    }
                    Lookup::ShardPut(put, index) => {
                        self.shard_holder(swarm, service, put, index, to);
                        self.finish_puts(swarm, service, Instant::now());
                    }
                    Lookup::ShardGet(read, index) => {
                        if to != *swarm.local_peer_id() && self.trust.trusted(&to) {
                            self.media_queue
                                .push_back((to, MediaJob::ShardGet(read, index)));
                            self.drain_media(swarm, service);
                        }
                    }
                    Lookup::ProfileRepair(record) => {
                        match self.profile_inventory(swarm, service, to, record) {
                            Ok(Some(request)) => self.republish.request(job, request),
                            Ok(None) => {}
                            Err(()) => self.republish.fail(job),
                        }
                    }
                    _ => unreachable!(),
                }
                return;
            }
            let mut peers = ok.peers.into_iter();
            if let Some(node) = peers.find(|p| {
                dyapp_p2p_net::id_has_pow(&p.peer_id, bits) && self.trust.trusted(&p.peer_id)
            }) {
                if let Some(request) = after_lookup(swarm, service, self, lookup, ok.key, node) {
                    self.republish.request(job, request);
                }
            }
        }
    }

    /// A peer's mailbox reply: confirms a put or answers an inventory with what it lacks.
    pub(super) fn mailbox_response(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        id: PeerId,
        request_id: OutboundRequestId,
        response: proto::MailboxResponse,
    ) {
        self.trust.add(id, 1);
        if let Some(put) = self.mailbox_puts.remove(&request_id) {
            if let Some(put) = self.puts.get_mut(&put) {
                put.confirm(id, response.status, None);
            }
            self.finish_puts(swarm, service, Instant::now());
        }
        if let Some(mailbox) = self.inventories.remove(&request_id) {
            let ok = response.status == i32::from(Status::Ok);
            let missing: HashSet<Vec<u8>> = response.missing.into_iter().collect();
            // shortcut: the replica puts sent here are not awaited, so a holder that drops them
            // still counts as handed over; track their replies if republished copies go missing.
            let complete =
                send_envelopes(swarm, service, &mut self.cap, id, &mailbox, |envelope| {
                    missing.contains(envelope)
                });
            self.republish.answered(request_id, ok && complete);
        }
    }

    /// A mailbox request to a peer failed.
    pub(super) fn mailbox_failure(
        &mut self,
        peer: PeerId,
        request_id: OutboundRequestId,
        error: &OutboundFailure,
    ) {
        // An old node lacking the protocol is no failure.
        if !matches!(error, OutboundFailure::UnsupportedProtocols) {
            self.trust.add(peer, -1);
        }
        let unsupported = matches!(error, OutboundFailure::UnsupportedProtocols);
        self.republish.answered(request_id, unsupported);
        self.inventories.remove(&request_id);
        self.mailbox_puts.remove(&request_id);
    }

    /// A peer's profile reply: fills its inventory gap or confirms a put.
    pub(super) fn profile_response(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        id: PeerId,
        request_id: OutboundRequestId,
        response: proto::ProfileResponse,
    ) {
        service.traffic.add(response.encoded_len() as u64);
        self.trust.add(id, 1);
        let refused = [Status::RateLimited, Status::Full].map(i32::from);
        let job = self
            .republish
            .answered(request_id, !refused.contains(&response.status));
        if let Some(record) = self.profile_inventories.remove(&request_id) {
            match self.profile_gap(swarm, service, id, &record, &response) {
                Ok(Some(request)) => self.republish.request(job, request),
                Ok(None) => {}
                Err(()) => self.republish.fail(job),
            }
        }
        if let Some(put) = self.profile_puts.remove(&request_id) {
            if let Some(put) = self.puts.get_mut(&put) {
                put.confirm(id, response.status, response.record);
            }
            self.finish_puts(swarm, service, Instant::now());
        }
    }

    /// A profile request to a peer failed.
    pub(super) fn profile_failure(
        &mut self,
        peer: PeerId,
        request_id: OutboundRequestId,
        error: &OutboundFailure,
    ) {
        if !matches!(error, OutboundFailure::UnsupportedProtocols) {
            self.trust.add(peer, -1);
        }
        let unsupported = matches!(error, OutboundFailure::UnsupportedProtocols);
        self.republish.answered(request_id, unsupported);
        self.profile_puts.remove(&request_id);
        self.profile_inventories.remove(&request_id);
    }

    /// A peer's KeyPackage replica reply confirms a publication.
    pub(super) fn keypackage_response(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        peer: PeerId,
        request_id: OutboundRequestId,
        response: proto::KeyPackageResponse,
    ) {
        service.traffic.add(response.encoded_len() as u64);
        self.trust.add(peer, 1);
        if let Some(id) = self.keypackage_puts.remove(&request_id) {
            if let Some(put) = self.puts.get_mut(&id) {
                put.confirm(peer, response.status, None);
            }
            self.finish_puts(swarm, service, Instant::now());
        }
    }

    /// A peer's KeyPackage request failed.
    pub(super) fn keypackage_failure(
        &mut self,
        peer: PeerId,
        request_id: OutboundRequestId,
        error: &OutboundFailure,
    ) {
        if !matches!(error, OutboundFailure::UnsupportedProtocols) {
            self.trust.add(peer, -1);
        }
        self.keypackage_puts.remove(&request_id);
    }

    /// A peer's media reply: a repair, shard or whole-copy answer.
    pub(super) fn media_response(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        id: PeerId,
        request_id: OutboundRequestId,
        response: proto::MediaResponse,
    ) {
        self.media_outgoing.remove(&request_id);
        if let Some((repair, job, reserve)) = self.media_repair_requests.remove(&request_id) {
            service
                .traffic
                .add((response.encoded_len() as u64).saturating_sub(reserve));
            if let Some((repair, _)) = self.media_repairs.get_mut(&repair) {
                repair.response(job, Some(response));
            }
            self.trust.add(id, 1);
            self.finish_puts(swarm, service, Instant::now());
            return;
        }
        let prepaid = self.shard_gets.contains_key(&request_id);
        service
            .traffic
            .add((response.encoded_len() - if prepaid { response.data.len() } else { 0 }) as u64);
        self.trust.add(id, 1);
        if let Some((put, index)) = self.shard_puts.remove(&request_id) {
            if response.status == i32::from(Status::Ok) {
                if let Some(put) = self.puts.get_mut(&put) {
                    put.shard_stored.insert(index);
                }
            }
        }
        if let Some((read, index)) = self.shard_gets.remove(&request_id) {
            if let Some(read) = self.media_reads.get_mut(&read) {
                if response.status == i32::from(Status::Ok)
                    && read.manifest.accepts(index, &response.data)
                {
                    read.shards.insert(index, response.data.clone());
                }
            }
        }
        if let Some(put) = self.media_puts.remove(&request_id) {
            if let Some(put) = self.puts.get_mut(&put) {
                put.confirm(id, response.status, None);
            }
        }
        self.finish_puts(swarm, service, Instant::now());
    }

    /// A media request to a peer failed.
    pub(super) fn media_failure(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        peer: PeerId,
        request_id: OutboundRequestId,
        error: &OutboundFailure,
    ) {
        if !matches!(error, OutboundFailure::UnsupportedProtocols) {
            self.trust.add(peer, -1);
        }
        if let Some((repair, job, _)) = self.media_repair_requests.remove(&request_id) {
            if let Some((repair, _)) = self.media_repairs.get_mut(&repair) {
                repair.response(job, None);
            }
        }
        self.media_puts.remove(&request_id);
        self.shard_puts.remove(&request_id);
        self.shard_gets.remove(&request_id);
        self.media_outgoing.remove(&request_id);
        self.finish_puts(swarm, service, Instant::now());
    }
}
