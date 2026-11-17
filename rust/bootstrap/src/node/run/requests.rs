use super::{
    enqueue, jobs, mailbox_request, peer, proto, Behaviour, ConnectionId, Job, PeerId, Replicas,
    ResponseChannel, Service, Status, StoreDone, Swarm,
};
use prost::Message as _;
use std::sync::Arc;

type StoreQueue = jobs::FairQueue<(ConnectionId, Job)>;

pub(super) fn profile(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    queue: &mut StoreQueue,
    permits: &Arc<tokio::sync::Semaphore>,
    (id, connection_id): (PeerId, ConnectionId),
    request: proto::ProfileRequest,
    channel: ResponseChannel<proto::ProfileResponse>,
) {
    let from = peer(swarm, service, id, connection_id);
    let permits = permits.clone();
    enqueue(
        queue,
        id,
        connection_id,
        Box::new(move |service, _| {
            let check_pow = |check: &crate::pow::PowCheck| {
                let _permit = permits.clone().try_acquire_owned().ok()?;
                Some(service.pow.check(check))
            };
            if let Some(
                proto::profile_request::Request::Publish(record)
                | proto::profile_request::Request::ConditionalPublish(record),
            ) = &request.request
            {
                let result =
                    service.prepare_profile_request_with_pow(&from, request.clone(), &check_pow);
                return StoreDone::ProfilePublish(channel, record.clone(), result);
            }
            let heartbeat = match &request.request {
                Some(proto::profile_request::Request::Heartbeat(record)) => {
                    Some(record.public_key.clone())
                }
                _ => None,
            };
            let result = service.profile_with_pow(&from, request, &check_pow);
            let ok = matches!(&result, Ok(response) if response.status == i32::from(Status::Ok));
            // An accepted heartbeat repairs the stored profile's replicas.
            let stored = heartbeat.filter(|_| ok).and_then(|key| {
                let owner = dyapp_identity::key_hash(&key.as_slice().try_into().ok()?);
                let owner = dyapp_identity::hex(&owner);
                service.store.get_profile(&owner).unwrap_or_else(|error| {
                    tracing::error!(%error, "heartbeat repair failed");
                    None
                })
            });
            StoreDone::Profile(channel, result, stored)
        }),
    );
}

pub(super) fn keypackage(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    queue: &mut StoreQueue,
    id: PeerId,
    connection_id: ConnectionId,
    request: proto::KeyPackageRequest,
    channel: ResponseChannel<proto::KeyPackageResponse>,
) {
    let from = peer(swarm, service, id, connection_id);
    enqueue(
        queue,
        id,
        connection_id,
        Box::new(move |service, _| {
            if let Some(proto::key_package_request::Request::Publish(record)) = &request.request {
                let result = service.prepare_keypackage(&from, record.clone());
                return StoreDone::KeyPackagePublish(channel, record.clone(), result);
            }
            StoreDone::KeyPackage(channel, service.keypackage(&from, request))
        }),
    );
}

pub(super) fn media(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    queue: &mut StoreQueue,
    replicas: &Replicas,
    (id, connection_id): (PeerId, ConnectionId),
    request: proto::MediaRequest,
    channel: ResponseChannel<proto::MediaResponse>,
) -> bool {
    let put = matches!(request.request, Some(proto::media_request::Request::Put(_)));
    if put && replicas.at_capacity() {
        let response = crate::service::media_upload_status(Status::Full);
        service.traffic.add(response.encoded_len() as u64);
        let _ = swarm.behaviour_mut().media.send_response(channel, response);
        return true;
    }
    let from = peer(swarm, service, id, connection_id);
    enqueue(
        queue,
        id,
        connection_id,
        Box::new(move |service, _| {
            if let Some(proto::media_request::Request::Put(put)) = &request.request {
                return StoreDone::MediaPut(channel, service.prepare_media(&from, put));
            }
            let keep = match &request.request {
                Some(proto::media_request::Request::Keep(record)) => Some(record.clone()),
                _ => None,
            };
            let have = match &request.request {
                Some(proto::media_request::Request::Have(have)) => Some(have.items.clone()),
                _ => None,
            };
            let get = match &request.request {
                Some(proto::media_request::Request::Get(get)) => Some(get.clone()),
                _ => None,
            };
            let result = service.media(&from, request);
            let missing =
                matches!(&result, Ok(response) if response.status == i32::from(Status::NotFound));
            let manifest = match (get, &service.media) {
                (Some(get), Some(media)) if missing => media
                    .manifest(&get.hash)
                    .ok()
                    .flatten()
                    .map(|manifest| (manifest, get)),
                _ => None,
            };
            StoreDone::Media {
                channel,
                result,
                keep,
                have,
                manifest,
            }
        }),
    );
    false
}

pub(super) fn mailbox(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    queue: &mut StoreQueue,
    id: PeerId,
    connection_id: ConnectionId,
    request: proto::MailboxRequest,
    channel: ResponseChannel<proto::MailboxResponse>,
) {
    let from = peer(swarm, service, id, connection_id);
    enqueue(
        queue,
        id,
        connection_id,
        Box::new(move |service, nonce| {
            if let Some(mailbox_request::Request::Put(record)) = &request.request {
                let result = service.prepare_mailbox(&from, record.clone());
                let group = service.put_group(record, &from);
                return StoreDone::MailboxPut(channel, record.clone(), group, result);
            }
            let kind = request.request.clone();
            StoreDone::Mailbox(channel, kind, service.mailbox(&from, nonce, request))
        }),
    );
}

pub(super) fn inventory(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    queue: &mut StoreQueue,
    id: PeerId,
    connection_id: ConnectionId,
    request: proto::InventoryRequest,
    channel: ResponseChannel<proto::InventoryResponse>,
) {
    let from = peer(swarm, service, id, connection_id);
    enqueue(
        queue,
        id,
        connection_id,
        Box::new(move |service, _| {
            StoreDone::Inventory(channel, service.search_inventory(&from, request))
        }),
    );
}

pub(super) fn digest(
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    queue: &mut StoreQueue,
    id: PeerId,
    connection_id: ConnectionId,
    request: proto::DigestRequest,
    channel: ResponseChannel<proto::DigestResponse>,
) {
    let from = peer(swarm, service, id, connection_id);
    enqueue(
        queue,
        id,
        connection_id,
        Box::new(move |service, _| {
            StoreDone::Digest(channel, service.search_digest(&from, request))
        }),
    );
}
