//! Every external mutation passes common admission before request-specific validation.
use super::*;

#[derive(Debug)]
enum Write {
    Profile(profile_request::Request),
    Mailbox(mailbox_request::Request),
    KeyPackage(proto::key_package_request::Request),
    Media(media_request::Request),
    Search(search_request::Request),
}

impl Write {
    fn search(service: &Service, peer: &Peer, request: search_request::Request) -> i32 {
        match service
            .search(
                peer,
                SearchRequest {
                    request: Some(request),
                },
            )
            .unwrap()
        {
            Search::Reply(response) => response.status,
            _ => panic!("default invalid write unexpectedly started work"),
        }
    }

    fn call(self, service: &Service, peer: &Peer) -> Status {
        let status = match self {
            Self::Profile(request) => {
                service
                    .profile(
                        peer,
                        ProfileRequest {
                            request: Some(request),
                            age_seconds: 0,
                        },
                    )
                    .unwrap()
                    .status
            }
            Self::Mailbox(request) => {
                service
                    .mailbox(
                        peer,
                        &mut None,
                        MailboxRequest {
                            request: Some(request),
                        },
                    )
                    .unwrap()
                    .status
            }
            Self::KeyPackage(request) => {
                service
                    .keypackage(
                        peer,
                        proto::KeyPackageRequest {
                            request: Some(request),
                        },
                    )
                    .unwrap()
                    .status
            }
            Self::Media(request) => {
                service
                    .media(
                        peer,
                        MediaRequest {
                            request: Some(request),
                        },
                    )
                    .unwrap()
                    .status
            }
            Self::Search(request) => Self::search(service, peer, request),
        };
        Status::try_from(status).unwrap()
    }
}

fn writes() -> Vec<Write> {
    use dyapp_identity::SignedRecord;
    let record = SignedRecord::default();
    vec![
        Write::Profile(profile_request::Request::Publish(record.clone())),
        Write::Profile(profile_request::Request::Heartbeat(record.clone())),
        Write::Profile(profile_request::Request::ReplicaPut(record.clone())),
        Write::Profile(profile_request::Request::ConditionalPublish(record.clone())),
        Write::Mailbox(mailbox_request::Request::Put(record.clone())),
        Write::Mailbox(mailbox_request::Request::Ack(record.clone())),
        Write::Mailbox(mailbox_request::Request::ReplicaAck(record.clone())),
        Write::Mailbox(mailbox_request::Request::ReplicaPut(
            proto::Envelopes::default(),
        )),
        Write::Mailbox(mailbox_request::Request::Revoke(record.clone())),
        Write::Mailbox(mailbox_request::Request::ReplicaRevoke(record.clone())),
        Write::KeyPackage(proto::key_package_request::Request::Publish(record.clone())),
        Write::KeyPackage(proto::key_package_request::Request::ReplicaPut(
            record.clone(),
        )),
        Write::Media(media_request::Request::Keep(record.clone())),
        Write::Media(media_request::Request::Put(proto::MediaPut::default())),
        Write::Media(media_request::Request::Attach(record.clone())),
        Write::Media(media_request::Request::Release(
            proto::MediaRelease::default(),
        )),
        Write::Media(media_request::Request::ReplicaPut(
            proto::MediaReplicaPut::default(),
        )),
        Write::Media(media_request::Request::ShardPut(
            proto::MediaShardPut::default(),
        )),
        Write::Media(media_request::Request::ConditionalKeep(record.clone())),
        Write::Media(media_request::Request::KeepCatchup(
            proto::MediaKeepCatchup::default(),
        )),
        Write::Search(search_request::Request::Publish(record.clone())),
        Write::Search(search_request::Request::Heartbeat(record.clone())),
        Write::Search(search_request::Request::Withdraw(record)),
    ]
}

fn admission_service() -> Service {
    let (_service_test_dir, mut service) = search_service();
    service.media = Some(MediaStore::open(&service.config.storage.dir).unwrap());
    service
}

#[test]
fn every_external_write_variant_is_denied_and_rate_limited_before_validation() {
    let service = admission_service();
    for (index, write) in writes().into_iter().enumerate() {
        let peer = replica_peer(&format!("listed-{index}"));
        service.deny.write().unwrap().insert(peer.id.clone());
        let label = format!("{write:?}");
        assert_eq!(write.call(&service, &peer), Status::Refused, "{label}");
    }

    let service = admission_service();
    service.traffic.add(service.config.limits.bytes_per_second);
    for write in writes() {
        let label = format!("{write:?}");
        assert_eq!(
            write.call(&service, &replica_peer("limited")),
            Status::RateLimited,
            "{label}"
        );
    }
}
