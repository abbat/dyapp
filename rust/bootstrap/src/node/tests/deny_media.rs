use super::super::replicas::listed_media_request;
use super::*;
use crate::{media::MediaStore, replication::media_encode, BootstrapStore, NodeConfig};
use dyapp_p2p_net::proto::{media_request::Request, MediaRequest};

fn service() -> Service {
    let dir = std::path::PathBuf::from(format!(
        "{}/../../build/test-deny-{}",
        env!("CARGO_MANIFEST_DIR"),
        uuid::Uuid::new_v4()
    ));
    let mut config = NodeConfig::default();
    config.storage.dir = dir.clone();
    config.limits.min_free_mb = 0;
    let mut service = Service::new(BootstrapStore::new(dir.to_str().unwrap()).unwrap(), config);
    service.media = Some(MediaStore::open(&dir).unwrap());
    service
}

fn hold(media: &MediaStore) -> (Vec<u8>, proto::MediaManifest) {
    let whole = b"listed whole blob".to_vec();
    let whole_hash = dyapp_identity::sha256(&whole).to_vec();
    let (manifest, shards) = media_encode(&vec![7; (1 << 20) + 1]).unwrap();
    let hashes = vec![whole_hash.clone(), manifest.hash.clone()];
    assert_eq!(media.keep("owner", 1, &hashes, 1).unwrap(), Some(hashes));
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
    (whole_hash, manifest)
}

fn refused(service: &Service, whole_hash: &[u8], manifest: &proto::MediaManifest) {
    let peer = Peer {
        id: "reader".into(),
        group: "group".into(),
        coarse: "coarse".into(),
    };
    for request in [
        Request::Get(proto::GetMedia {
            hash: whole_hash.to_vec(),
            ..Default::default()
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
}

fn purges(db: &rusqlite::Connection) -> i64 {
    db.query_row("SELECT `count` FROM `purges` WHERE `id` = 1", [], |r| {
        r.get(0)
    })
    .unwrap()
}

#[test]
fn reloading_deny_purges_whole_and_sharded_blobs_without_dropping_keeps() {
    let service = service();
    let media = service.media.as_ref().unwrap();
    let (whole_hash, manifest) = hold(media);
    let hashes = vec![whole_hash.clone(), manifest.hash.clone()];
    let deny = DenyStore::open(&service.config.storage.deny_path()).unwrap();
    for hash in &hashes {
        deny.add(&dyapp_identity::hex(hash), "operator decision")
            .unwrap();
    }
    let mut version = None;
    reload_deny(&service, &deny, &mut version);
    assert!(version.is_some());
    assert_eq!(media.get(&whole_hash).unwrap(), None);
    assert_eq!(media.manifest(&manifest.hash).unwrap(), None);
    assert_eq!(media.shard(&manifest.hash, 0).unwrap(), None);
    assert_eq!(media.keep("owner", 2, &hashes, 2).unwrap(), Some(hashes));
    refused(&service, &whole_hash, &manifest);
    let db = rusqlite::Connection::open(service.config.storage.media_path()).unwrap();
    assert_eq!(purges(&db), 2);
    reload_deny(&service, &deny, &mut version);
    assert_eq!(
        purges(&db),
        2,
        "an unchanged deny list does not recount purges"
    );
}

#[test]
fn outgoing_media_requests_skip_listed_blobs() {
    let service = service();
    let data = b"repair payload".to_vec();
    let hash = dyapp_identity::sha256(&data);
    let entry: String = dyapp_identity::hex(&hash);
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
