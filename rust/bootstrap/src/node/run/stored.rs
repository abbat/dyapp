//! Replies to store jobs finished off the loop, and the replication they start.

use super::*;

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
    let answered = match done {
        StoreDone::ProfilePublish(channel, record, result) => match result {
            Ok(response) if response.status == i32::from(Status::Ok) => {
                replicas.start_put(
                    swarm,
                    service,
                    watchers,
                    PutRecord::Profile(record),
                    PutReply::Profile(channel),
                );
                true
            }
            Ok(response) => {
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm
                    .behaviour_mut()
                    .profile
                    .send_response(channel, response);
                true
            }
            Err(error) => {
                tracing::error!(%error, "profile admission failed");
                false
            }
        },
        StoreDone::Profile(channel, result, stored) => match result {
            Ok(response) => {
                if let Some(record) = stored {
                    replicas.repair_profile(swarm, service, record, Instant::now());
                }
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm
                    .behaviour_mut()
                    .profile
                    .send_response(channel, response);
                true
            }
            // Dropping the channel fails the request; the client tries another node.
            Err(error) => {
                tracing::error!(%error, "profile request failed");
                false
            }
        },
        StoreDone::KeyPackagePublish(channel, record, result) => match result {
            Ok(response) if response.status == i32::from(Status::Ok) => {
                replicas.start_put(
                    swarm,
                    service,
                    watchers,
                    PutRecord::KeyPackage(record),
                    PutReply::KeyPackage(channel),
                );
                true
            }
            Ok(response) => {
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm
                    .behaviour_mut()
                    .key_package
                    .send_response(channel, response);
                true
            }
            Err(error) => {
                tracing::error!(%error, "KeyPackage admission failed");
                false
            }
        },
        StoreDone::KeyPackage(channel, result) => match result {
            Ok(response) => {
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm
                    .behaviour_mut()
                    .key_package
                    .send_response(channel, response);
                true
            }
            Err(error) => {
                tracing::error!(%error, "KeyPackage request failed");
                false
            }
        },
        StoreDone::MediaPut(channel, result) => match result {
            Ok(Ok(replica)) => {
                replicas.start_put(
                    swarm,
                    service,
                    watchers,
                    PutRecord::Media(replica),
                    PutReply::Media(channel),
                );
                true
            }
            Ok(Err(status)) => {
                let response = crate::service::media_upload_status(status);
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm.behaviour_mut().media.send_response(channel, response);
                true
            }
            Err(error) => {
                tracing::error!(%error, "media admission failed");
                false
            }
        },
        StoreDone::Media {
            channel,
            result,
            keep,
            have,
            manifest,
        } => match result {
            Ok(mut response) => {
                if response.status == i32::from(Status::Ok) {
                    if let Some(items) = have {
                        response = replicas
                            .media_inventory(swarm, service, id, &items)
                            .unwrap_or_else(|error| {
                                tracing::error!(%error, "media inventory failed");
                                proto::MediaResponse {
                                    status: Status::NotFound.into(),
                                    ..Default::default()
                                }
                            });
                        let _ = swarm.behaviour_mut().media.send_response(channel, response);
                        return true;
                    }
                    if let Some(record) = keep {
                        replicas.repair_media(
                            swarm,
                            service,
                            record,
                            channel,
                            response,
                            Instant::now(),
                        );
                        return true;
                    }
                }
                if let Some((manifest, get)) = manifest {
                    let from = peer(swarm, service, id, connection);
                    if replicas.media_reads.len() >= 4
                        || !service.prepare_media_read(&from, &manifest, &get)
                    {
                        PutReply::Media(channel).send(swarm, service, Status::RateLimited, None);
                    } else {
                        replicas.start_read(swarm, service, manifest, get, channel);
                    }
                    return true;
                }
                // A get's payload was reserved by the service before returning it.
                service
                    .traffic
                    .add((response.encoded_len() - response.data.len()) as u64);
                let _ = swarm.behaviour_mut().media.send_response(channel, response);
                true
            }
            Err(error) => {
                tracing::error!(%error, "media request failed");
                false
            }
        },
        StoreDone::MailboxPut(channel, record, group, result) => match result {
            Ok(response) if response.status == i32::from(Status::Ok) => {
                replicas.start_put(
                    swarm,
                    service,
                    watchers,
                    PutRecord::Mailbox(record, group),
                    PutReply::Mailbox(channel),
                );
                true
            }
            Ok(response) => {
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm
                    .behaviour_mut()
                    .mailbox
                    .send_response(channel, response);
                true
            }
            Err(error) => {
                tracing::error!(%error, "mailbox admission failed");
                false
            }
        },
        StoreDone::Mailbox(channel, kind, result) => match result {
            Ok(response) => {
                let ok = response.status == i32::from(Status::Ok);
                let signed = matches!(
                    kind,
                    Some(mailbox_request::Request::ReplicaPut(_))
                        | Some(mailbox_request::Request::ReplicaAck(_))
                        | Some(mailbox_request::Request::ReplicaRevoke(_))
                );
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm
                    .behaviour_mut()
                    .mailbox
                    .send_response(channel, response);
                if ok && signed {
                    replicas.trust.add(id, 1);
                }
                if ok {
                    after_mailbox(swarm, service, watchers, replicas, (id, connection), kind);
                }
                true
            }
            Err(error) => {
                tracing::error!(%error, "mailbox request failed");
                false
            }
        },
        StoreDone::Inventory(channel, result) => match result {
            Ok(response) => {
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm
                    .behaviour_mut()
                    .inventory
                    .send_response(channel, response);
                true
            }
            Err(error) => {
                tracing::error!(%error, "inventory request failed");
                false
            }
        },
        StoreDone::Digest(channel, result) => match result {
            Ok(response) => {
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm
                    .behaviour_mut()
                    .digest
                    .send_response(channel, response);
                true
            }
            Err(error) => {
                tracing::error!(%error, "digest request failed");
                false
            }
        },
    };
    drop_banned(swarm, service, id);
    answered
}

/// The hourly expiry of messages, KeyPackages, inactive profiles, attachments, media keeps, idle
/// assembled media and search entries.
pub(super) fn expire(service: &Service) {
    let now = chrono::Utc::now().timestamp();
    match service.store.cleanup_expired(now) {
        Ok(removed) => tracing::info!(removed, "expired messages removed"),
        Err(error) => tracing::error!(%error, "message cleanup failed"),
    }
    if let Err(error) = service.store.expire_keypackages(now) {
        tracing::error!(%error, "KeyPackage cleanup failed");
    }
    let ttl = i64::from(service.config.limits.profile_ttl_days) * 86_400;
    match service.store.expire_profiles(now - ttl) {
        Ok(removed) => tracing::info!(removed, "profiles of inactive owners removed"),
        Err(error) => tracing::error!(%error, "profile cleanup failed"),
    }
    if let Some(media) = &service.media {
        match media.expire(now.unsigned_abs()) {
            Ok(removed) => tracing::info!(removed, "expired attachments removed"),
            Err(error) => tracing::error!(%error, "attachment cleanup failed"),
        }
        match media.expire_owners(now - ttl) {
            Ok(removed) => tracing::info!(removed, "media keeps of inactive owners removed"),
            Err(error) => tracing::error!(%error, "media owner cleanup failed"),
        }
        let idle = i64::from(service.config.limits.media_cache_minutes) * 60;
        match media.expire_cache(now - idle) {
            Ok(0) => {}
            Ok(removed) => tracing::info!(removed, "idle assembled media removed"),
            Err(error) => tracing::error!(%error, "media cache cleanup failed"),
        }
    }
    if let Some(index) = &service.search {
        match index.expire(now - ttl) {
            Ok(0) => {}
            Ok(removed) => tracing::info!(removed, "search entries of silent owners removed"),
            Err(error) => tracing::error!(%error, "search cleanup failed"),
        }
    }
}
