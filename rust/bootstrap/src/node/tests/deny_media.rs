use super::super::replicas::listed_media_request;
use super::*;
use crate::{media::MediaStore, replication::media_encode, BootstrapStore, NodeConfig};
use dyapp_p2p_net::proto::{media_request::Request, MediaRequest};

#[test]
fn reloading_deny_purges_whole_and_sharded_blobs_without_dropping_keeps() {
    let dir = std::path::PathBuf::from(format!("/tmp/ai/test-deny-purge-{}", uuid::Uuid::new_v4()));
    let mut config = NodeConfig::default();
    config.storage.dir = dir.clone();
    let mut service = Service::new(BootstrapStore::new(dir.to_str().unwrap()).unwrap(), config);
    service.media = Some(MediaStore::open(&dir).unwrap());
    let media = service.media.as_ref().unwrap();
    let whole = b"listed whole blob".to_vec();
    let whole_hash = dyapp_identity::sha256(&whole).to_vec();
    let (manifest, shards) = media_encode(&vec![7; (1 << 20) + 1]).unwrap();
    let hashes = vec![whole_hash.clone(), manifest.hash.clone()];
    assert_eq!(
        media.keep("owner", 1, &hashes, 1).unwrap(),
        Some(hashes.clone())
    );
    assert_eq!(
        media.put("owner", &whole, 20 << 20).unwrap(),
        crate::media::Put::Stored
    );
    assert_eq!(
        media.put_manifest("owner", &manifest, 20 << 20).unwrap(),
        crate::media::Put::Stored
    );
    assert_eq!(
        media.put_shard(&manifest, 0, &shards[0]).unwrap(),
        crate::media::Put::Stored
    );

    let deny = DenyStore::open(&service.config.storage.deny_path()).unwrap();
    for hash in &hashes {
        let entry: String = hash.iter().map(|byte| format!("{byte:02x}")).collect();
        deny.add(&entry, "operator decision").unwrap();
    }
    let mut version = None;
    reload_deny(&service, &deny, &mut version);
    assert!(version.is_some());
    assert_eq!(media.get(&whole_hash).unwrap(), None);
    assert_eq!(media.manifest(&manifest.hash).unwrap(), None);
    assert_eq!(media.shard(&manifest.hash, 0).unwrap(), None);
    assert_eq!(
        media.keep("owner", 2, &hashes, 2).unwrap(),
        Some(hashes.clone())
    );

    let peer = Peer {
        id: "reader".into(),
        group: "group".into(),
        coarse: "coarse".into(),
    };
    for request in [
        Request::Get(proto::GetMedia {
            hash: whole_hash.clone(),
        }),
        Request::ShardGet(proto::MediaShardGet {
            hash: manifest.hash.clone(),
            index: 0,
        }),
    ] {
        assert_eq!(
            service
                .media(
                    &peer,
                    MediaRequest {
                        request: Some(request)
                    }
                )
                .unwrap()
                .status,
            i32::from(Status::Refused)
        );
    }
    let db = rusqlite::Connection::open(service.config.storage.media_path()).unwrap();
    let count: i64 = db
        .query_row("SELECT `count` FROM `purges` WHERE `id` = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 2);
    reload_deny(&service, &deny, &mut version);
    let count: i64 = db
        .query_row("SELECT `count` FROM `purges` WHERE `id` = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 2, "an unchanged deny list does not recount purges");
}

#[test]
fn outgoing_media_requests_skip_listed_blobs() {
    let dir = format!("/tmp/ai/test-deny-outgoing-{}", uuid::Uuid::new_v4());
    let service = Service::new(BootstrapStore::new(&dir).unwrap(), NodeConfig::default());
    let data = b"repair payload".to_vec();
    let hash = dyapp_identity::sha256(&data);
    let entry: String = hash.iter().map(|byte| format!("{byte:02x}")).collect();
    service.deny.write().unwrap().insert(entry);
    assert!(listed_media_request(
        &service,
        &Request::ReplicaPut(proto::MediaReplicaPut {
            data,
            ..Default::default()
        })
    ));
    assert!(listed_media_request(
        &service,
        &Request::ShardGet(proto::MediaShardGet {
            hash: hash.to_vec(),
            index: 0
        })
    ));
    assert!(!listed_media_request(
        &service,
        &Request::ShardGet(proto::MediaShardGet {
            hash: vec![0; 32],
            index: 0
        })
    ));
}
