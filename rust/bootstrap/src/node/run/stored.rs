//! Replies to store jobs finished off the loop, and the replication they start.

use super::*;
use mailbox_request::Request::{ReplicaAck, ReplicaPut, ReplicaRevoke};

/// Replies to finished store job `done` of `peer` and starts what follows it: a put's
/// replication, a profile or media repair, a mailbox watch. Returns false when the store
/// failed, so the request was not answered.
pub(super) fn reply(
    swarm: &mut Swarm<Behaviour>,
    service: &Arc<Service>,
    replicas: &mut Replicas,
    watchers: &mut Watchers,
    (id, connection): (PeerId, ConnectionId),
    done: StoreDone,
) -> bool {
    let answered = Replies {
        swarm,
        service,
        replicas,
        watchers,
        id,
        connection,
    }
    .send(done);
    drop_banned(swarm, service, id);
    answered
}

fn committed_keep(response: &proto::MediaResponse) -> Option<SignedRecord> {
    if !response.keep_applied {
        return None;
    }
    response.keep_state.as_ref()?.record.clone()
}

type Channel<T> = libp2p::request_response::ResponseChannel<T>;

struct Replies<'a> {
    swarm: &'a mut Swarm<Behaviour>,
    service: &'a Arc<Service>,
    replicas: &'a mut Replicas,
    watchers: &'a mut Watchers,
    id: PeerId,
    connection: ConnectionId,
}

fn completed<T>(result: crate::Result<T>, message: &str) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::error!(%error, "{message}");
            None
        }
    }
}

impl Replies<'_> {
    fn send(&mut self, done: StoreDone) -> bool {
        match done {
            StoreDone::ProfilePublish(channel, record, result) => {
                self.profile_publish(channel, record, result)
            }
            StoreDone::Profile(channel, result, stored) => self.profile(channel, result, stored),
            StoreDone::KeyPackagePublish(channel, record, result) => {
                self.keypackage_publish(channel, record, result)
            }
            StoreDone::KeyPackage(channel, result) => self.keypackage(channel, result),
            StoreDone::MediaPut(channel, result) => self.media_put(channel, result),
            StoreDone::Media {
                channel,
                result,
                keep,
                have,
                manifest,
            } => self.media(channel, result, keep, have, manifest),
            StoreDone::MailboxPut(channel, record, group, result) => {
                self.mailbox_put(channel, record, group, result)
            }
            StoreDone::Mailbox(channel, kind, result) => self.mailbox(channel, kind, result),
            StoreDone::Inventory(channel, result) => self.inventory(channel, result),
            StoreDone::Digest(channel, result) => self.digest(channel, result),
        }
    }

    fn profile_publish(
        &mut self,
        channel: Channel<proto::ProfileResponse>,
        record: SignedRecord,
        result: crate::Result<proto::ProfileResponse>,
    ) -> bool {
        let Some(response) = completed(result, "profile admission failed") else {
            return false;
        };
        if response.status == i32::from(Status::Ok) {
            self.replicas.start_put(
                self.swarm,
                self.service,
                self.watchers,
                PutRecord::Profile(record),
                PutReply::Profile(channel),
            );
        } else {
            self.service.traffic.add(response.encoded_len() as u64);
            let _ = self
                .swarm
                .behaviour_mut()
                .profile
                .send_response(channel, response);
        }
        true
    }

    fn profile(
        &mut self,
        channel: Channel<proto::ProfileResponse>,
        result: crate::Result<proto::ProfileResponse>,
        stored: Option<SignedRecord>,
    ) -> bool {
        let Some(response) = completed(result, "profile request failed") else {
            return false;
        };
        if let Some(record) = stored {
            self.replicas
                .repair_profile(self.swarm, self.service, record, Instant::now());
        }
        self.service.traffic.add(response.encoded_len() as u64);
        let _ = self
            .swarm
            .behaviour_mut()
            .profile
            .send_response(channel, response);
        true
    }

    fn keypackage_publish(
        &mut self,
        channel: Channel<proto::KeyPackageResponse>,
        record: SignedRecord,
        result: crate::Result<proto::KeyPackageResponse>,
    ) -> bool {
        let Some(response) = completed(result, "KeyPackage admission failed") else {
            return false;
        };
        if response.status == i32::from(Status::Ok) {
            self.replicas.start_put(
                self.swarm,
                self.service,
                self.watchers,
                PutRecord::KeyPackage(record),
                PutReply::KeyPackage(channel),
            );
        } else {
            self.service.traffic.add(response.encoded_len() as u64);
            let _ = self
                .swarm
                .behaviour_mut()
                .key_package
                .send_response(channel, response);
        }
        true
    }

    fn keypackage(
        &mut self,
        channel: Channel<proto::KeyPackageResponse>,
        result: crate::Result<proto::KeyPackageResponse>,
    ) -> bool {
        let Some(response) = completed(result, "KeyPackage request failed") else {
            return false;
        };
        self.service.traffic.add(response.encoded_len() as u64);
        let _ = self
            .swarm
            .behaviour_mut()
            .key_package
            .send_response(channel, response);
        true
    }

    fn media_put(
        &mut self,
        channel: Channel<proto::MediaResponse>,
        result: crate::Result<std::result::Result<proto::MediaReplicaPut, Status>>,
    ) -> bool {
        let Some(result) = completed(result, "media admission failed") else {
            return false;
        };
        match result {
            Ok(replica) => {
                self.replicas.start_put(
                    self.swarm,
                    self.service,
                    self.watchers,
                    PutRecord::Media(replica),
                    PutReply::Media(channel),
                );
            }
            Err(status) => {
                let response = crate::service::media_upload_status(status);
                self.service.traffic.add(response.encoded_len() as u64);
                let _ = self
                    .swarm
                    .behaviour_mut()
                    .media
                    .send_response(channel, response);
            }
        }
        true
    }

    fn media(
        &mut self,
        channel: Channel<proto::MediaResponse>,
        result: crate::Result<proto::MediaResponse>,
        keep: Option<SignedRecord>,
        have: Option<Vec<proto::MediaPart>>,
        manifest: Option<(proto::MediaManifest, proto::GetMedia)>,
    ) -> bool {
        let Some(response) = completed(result, "media request failed") else {
            return false;
        };
        if response.status == i32::from(Status::Ok) {
            if let Some(items) = have {
                return self.media_inventory(channel, &items);
            }
            let keep = keep.or_else(|| committed_keep(&response));
            if let Some(record) = keep {
                self.replicas.repair_media(
                    self.swarm,
                    self.service,
                    record,
                    channel,
                    response,
                    Instant::now(),
                );
                return true;
            }
        }
        if let Some((manifest, get)) = manifest {
            self.media_read(channel, manifest, get);
            return true;
        }
        // A get's payload was reserved by the service before returning it.
        self.service
            .traffic
            .add((response.encoded_len() - response.data.len()) as u64);
        let _ = self
            .swarm
            .behaviour_mut()
            .media
            .send_response(channel, response);
        true
    }

    fn media_inventory(
        &mut self,
        channel: Channel<proto::MediaResponse>,
        items: &[proto::MediaPart],
    ) -> bool {
        let response = completed(
            self.replicas
                .media_inventory(self.swarm, self.service, self.id, items),
            "media inventory failed",
        )
        .unwrap_or_else(|| proto::MediaResponse {
            status: Status::NotFound.into(),
            ..Default::default()
        });
        let _ = self
            .swarm
            .behaviour_mut()
            .media
            .send_response(channel, response);
        true
    }

    fn media_read(
        &mut self,
        channel: Channel<proto::MediaResponse>,
        manifest: proto::MediaManifest,
        get: proto::GetMedia,
    ) {
        let from = peer(self.swarm, self.service, self.id, self.connection);
        if self.replicas.media_reads.len() >= 4
            || !self.service.prepare_media_read(&from, &manifest, &get)
        {
            PutReply::Media(channel).send(self.swarm, self.service, Status::RateLimited, None);
        } else {
            self.replicas
                .start_read(self.swarm, self.service, manifest, get, channel);
        }
    }

    fn mailbox_put(
        &mut self,
        channel: Channel<proto::MailboxResponse>,
        record: SignedRecord,
        group: Vec<u8>,
        result: crate::Result<proto::MailboxResponse>,
    ) -> bool {
        let Some(response) = completed(result, "mailbox admission failed") else {
            return false;
        };
        if response.status == i32::from(Status::Ok) {
            self.replicas.start_put(
                self.swarm,
                self.service,
                self.watchers,
                PutRecord::Mailbox(record, group),
                PutReply::Mailbox(channel),
            );
        } else {
            self.service.traffic.add(response.encoded_len() as u64);
            let _ = self
                .swarm
                .behaviour_mut()
                .mailbox
                .send_response(channel, response);
        }
        true
    }

    fn mailbox(
        &mut self,
        channel: Channel<proto::MailboxResponse>,
        kind: Option<mailbox_request::Request>,
        result: crate::Result<proto::MailboxResponse>,
    ) -> bool {
        let Some(response) = completed(result, "mailbox request failed") else {
            return false;
        };
        let ok = response.status == i32::from(Status::Ok);
        let signed = matches!(kind, Some(ReplicaPut(_) | ReplicaAck(_) | ReplicaRevoke(_)));
        self.service.traffic.add(response.encoded_len() as u64);
        let _ = self
            .swarm
            .behaviour_mut()
            .mailbox
            .send_response(channel, response);
        if ok && signed {
            self.replicas.trust.add(self.id, 1);
        }
        if ok {
            after_mailbox(
                self.swarm,
                self.service,
                self.watchers,
                self.replicas,
                (self.id, self.connection),
                kind,
            );
        }
        true
    }

    fn inventory(
        &mut self,
        channel: Channel<proto::InventoryResponse>,
        result: crate::Result<proto::InventoryResponse>,
    ) -> bool {
        let Some(response) = completed(result, "inventory request failed") else {
            return false;
        };
        self.service.traffic.add(response.encoded_len() as u64);
        let _ = self
            .swarm
            .behaviour_mut()
            .inventory
            .send_response(channel, response);
        true
    }

    fn digest(
        &mut self,
        channel: Channel<proto::DigestResponse>,
        result: crate::Result<proto::DigestResponse>,
    ) -> bool {
        let Some(response) = completed(result, "digest request failed") else {
            return false;
        };
        self.service.traffic.add(response.encoded_len() as u64);
        let _ = self
            .swarm
            .behaviour_mut()
            .digest
            .send_response(channel, response);
        true
    }
}

/// The hourly expiry of messages, `KeyPackages`, inactive profiles, attachments, media keeps, idle
/// assembled media and search entries.
pub(super) fn expire(service: &Service) {
    let now = chrono::Utc::now().timestamp();
    removed(
        service.store.cleanup_expired(now),
        "expired messages removed",
        "message cleanup failed",
        false,
    );
    let _ = completed(
        service.store.expire_keypackages(now),
        "KeyPackage cleanup failed",
    );
    let ttl = i64::from(service.config.limits.profile_ttl_days) * 86_400;
    removed(
        service.store.expire_profiles(now - ttl),
        "profiles of inactive owners removed",
        "profile cleanup failed",
        false,
    );
    if let Some(media) = &service.media {
        removed(
            media.expire(now.unsigned_abs()),
            "expired attachments removed",
            "attachment cleanup failed",
            false,
        );
        removed(
            media.expire_owners(now - ttl),
            "media keeps of inactive owners removed",
            "media owner cleanup failed",
            false,
        );
        let idle = i64::from(service.config.limits.media_cache_minutes) * 60;
        removed(
            media.expire_cache(now - idle),
            "idle assembled media removed",
            "media cache cleanup failed",
            true,
        );
    }
    if let Some(index) = &service.search {
        removed(
            index.expire(now - ttl),
            "search entries of silent owners removed",
            "search cleanup failed",
            true,
        );
    }
}

fn removed<T: tracing::field::Value + PartialEq + From<u8>>(
    result: crate::Result<T>,
    success: &str,
    failure: &str,
    quiet_zero: bool,
) {
    let Some(removed) = completed(result, failure) else {
        return;
    };
    if quiet_zero && removed == T::from(0) {
        return;
    }
    tracing::info!(removed, "{success}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hourly_expiry_drops_idle_assembled_media_only() {
        let dir = format!("/tmp/ai/test-hourly-media-cache-{}", uuid::Uuid::new_v4());
        let mut config = crate::NodeConfig::default();
        config.storage.dir = dir.clone().into();
        let mut service = Service::new(crate::BootstrapStore::new(&dir).unwrap(), config);
        service.media = Some(crate::media::MediaStore::open(&service.config.storage.dir).unwrap());
        let media = service.media.as_ref().unwrap();
        let now = chrono::Utc::now().timestamp();
        let idle = i64::from(service.config.limits.media_cache_minutes) * 60;
        let old = [1; 32];
        let recent = [2; 32];
        media.cache(&old, b"old", now - idle - 60).unwrap();
        media.cache(&recent, b"recent", now).unwrap();

        expire(&service);

        assert_eq!(media.cached(&old, now).unwrap(), None);
        assert_eq!(
            media.cached(&recent, now).unwrap(),
            Some(b"recent".to_vec())
        );
    }
}
