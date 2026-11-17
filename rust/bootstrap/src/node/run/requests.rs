use super::{
    enqueue, jobs, mailbox_request, peer, proto, Behaviour, ConnectionId, Job, PeerId,
    ResponseChannel, Service, StoreDone, Swarm,
};

type StoreQueue = jobs::FairQueue<(ConnectionId, Job)>;

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
