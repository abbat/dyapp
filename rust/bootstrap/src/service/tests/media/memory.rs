//! Local media corruption must not turn a network request into an unbounded allocation.
use super::*;

fn get(s: &Service, hash: &[u8]) -> crate::Result<MediaResponse> {
    s.media(
        &peer("media-memory"),
        MediaRequest {
            request: Some(media_request::Request::Get(proto::GetMedia {
                hash: hash.to_vec(),
                offset: 0,
                len: 1,
            })),
        },
    )
}

#[test]
fn whole_and_cached_files_stop_at_the_media_limit_before_service_allocation() {
    let s = media_service(|limits| limits.bytes_per_second = 64 << 20);
    let owner = Identity::try_generate().unwrap();
    let data = b"whole".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    assert_eq!(
        s.store_media(&media_replica(&owner, data, 1))
            .unwrap()
            .status,
        i32::from(Status::Ok)
    );
    let whole = s.media.as_ref().unwrap().path(&hash);
    std::fs::write(&whole, vec![1; crate::replication::MEDIA_MAX_BYTES]).unwrap();
    assert_eq!(get(&s, &hash).unwrap().data, [1]);
    std::fs::write(&whole, vec![1; crate::replication::MEDIA_MAX_BYTES + 1]).unwrap();
    assert!(get(&s, &hash).is_err());

    let cached_hash = [8; 32];
    let cached = s
        .config
        .storage
        .dir
        .join("cache")
        .join(dyapp_identity::hex(&cached_hash));
    s.media
        .as_ref()
        .unwrap()
        .cache(&cached_hash, b"cached", 100)
        .unwrap();
    std::fs::write(&cached, vec![2; crate::replication::MEDIA_MAX_BYTES]).unwrap();
    assert_eq!(get(&s, &cached_hash).unwrap().data, [2]);
    std::fs::write(&cached, vec![2; crate::replication::MEDIA_MAX_BYTES + 1]).unwrap();
    assert!(get(&s, &cached_hash).is_err());
}

#[test]
fn shard_sql_excludes_oversized_values_before_service_allocation() {
    let s = media_service(|limits| limits.bytes_per_second = 64 << 20);
    let hash = [9; 32];
    let db = rusqlite::Connection::open(s.config.storage.dir.join("media.db")).unwrap();
    let request = || MediaRequest {
        request: Some(media_request::Request::ShardGet(proto::MediaShardGet {
            hash: hash.to_vec(),
            index: 0,
        })),
    };
    for size in [
        crate::replication::MEDIA_OBJECT_BYTES,
        crate::replication::MEDIA_OBJECT_BYTES + 1,
    ] {
        db.execute(
            "INSERT OR REPLACE INTO `shards` (`hash`, `idx`, `data`) VALUES (?, ?, ?)",
            rusqlite::params![hash, 0, vec![3u8; size]],
        )
        .unwrap();
        let response = s.media(&peer("media-memory-shard"), request()).unwrap();
        if size == crate::replication::MEDIA_OBJECT_BYTES {
            assert_eq!(response.data, vec![3u8; size]);
        } else {
            assert_eq!(
                (Status::try_from(response.status).unwrap(), response.data),
                (Status::NotFound, Vec::new())
            );
        }
    }
}
