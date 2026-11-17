use super::*;

#[test]
fn store_media_checks_each_condition_and_evicts_cache_before_refusing() {
    let s = media_service(|_| {});
    let owner = Identity::try_generate().unwrap();
    check_keep_conditions(&s, &owner);
    check_attachment_conditions(&s, &owner);
    check_cache_eviction_and_disk_space(&owner);
}

fn check_keep_conditions(s: &Service, owner: &Identity) {
    use proto::media_replica_put::Authorization;

    let now = || crate::clock::Clock::system().timestamp().unsigned_abs();
    // Manifest replicas carry their data in shards, never inline.
    let data = b"manifested".to_vec();
    let (manifest, _) = crate::replication::media_encode(&data).unwrap();
    let mut replica = media_replica(owner, data, 1);
    replica.manifest = Some(manifest);
    assert_eq!(store_media_status(s, &replica), Status::Invalid);
    let zero = media_replica(owner, b"version zero".to_vec(), 0);
    assert_eq!(store_media_status(s, &zero), Status::Invalid);
    let hashes = media_hashes;
    let keep = |data: &[u8], hashes: Vec<Vec<u8>>, time| {
        let keep = proto::MediaKeep {
            version: 1,
            hashes,
            time,
        };
        proto::MediaReplicaPut {
            data: data.to_vec(),
            manifest: None,
            authorization: Some(Authorization::Keep(
                Identity::try_generate().unwrap().sign(Domain::MediaKeep, keep.encode_to_vec()),
            )),
        }
    };
    // Each keep hash rule rejects on its own; exactly MAX_KEEP hashes are allowed.
    let mut bad = hashes(b"kept", 1);
    bad.push(vec![0; 31]);
    for (hashes, expected) in [
        (hashes(b"kept", MAX_KEEP), Status::Ok),
        (hashes(b"kept", MAX_KEEP + 1), Status::Invalid),
        (bad, Status::Invalid),
    ] {
        assert_eq!(
            store_media_status(s, &keep(b"kept", hashes, now())),
            expected
        );
    }
    // A keep exactly profile_ttl_days old is still live; retry if the second ticks over.
    let ttl = u64::from(s.config.limits.profile_ttl_days) * 86400;
    loop {
        let before = now();
        let result = store_media_status(s, &keep(b"aged", hashes(b"aged", 1), before - ttl));
        if now() == before {
            assert_eq!(result, Status::Ok);
            break;
        }
    }
}

fn check_attachment_conditions(s: &Service, owner: &Identity) {
    use proto::media_replica_put::Authorization;

    let now = || crate::clock::Clock::system().timestamp().unsigned_abs();
    let hashes = media_hashes;
    // Each attachment rule rejects on its own; exactly MAX_ATTACH hashes are allowed.
    let data = b"attached".to_vec();
    let hash = dyapp_identity::sha256(&data).to_vec();
    let release = |i: u8| dyapp_identity::sha256(&[i]).to_vec();
    let attach = |release, hashes, created| {
        let attach = proto::MediaAttach {
            release,
            hashes,
            created,
        };
        proto::MediaReplicaPut {
            data: data.clone(),
            manifest: None,
            authorization: Some(Authorization::Attach(
                owner.sign(Domain::MediaAttach, attach.encode_to_vec()),
            )),
        }
    };
    for (i, (release, hashes, expected)) in [
        (release(1), vec![hash.clone()], Status::Ok),
        (
            release(2)[..31].to_vec(),
            vec![hash.clone()],
            Status::Invalid,
        ),
        (release(3), Vec::new(), Status::Invalid),
        (release(4), hashes(&data, MAX_ATTACH + 1), Status::Invalid),
        (release(5), vec![hash.clone(), vec![0; 31]], Status::Invalid),
        (release(6), hashes(&data, MAX_ATTACH), Status::Ok),
    ]
    .into_iter()
    .enumerate()
    {
        // Two hours old stays within attachment_retention_hours.
        let replica = attach(release, hashes, now() - 7200);
        assert_eq!(store_media_status(s, &replica), expected, "case {i}");
    }
    let future = attach(release(7), vec![hash.clone()], now() + 3600);
    assert_eq!(store_media_status(s, &future), Status::Invalid);
    // A deny-listed owner alone refuses an attachment of an allowed hash.
    s.deny.write().unwrap().insert(owner.peer_id());
    let denied = attach(release(8), vec![hash.clone()], now());
    assert_eq!(store_media_status(s, &denied), Status::Refused);
}

fn check_cache_eviction_and_disk_space(owner: &Identity) {
    // A full store first evicts cached blobs, oldest read first, down to 95%.
    let s = media_service(|l| l.media_max_mb = 1);
    let media = s.media.as_ref().unwrap();
    for read in 1..=3u8 {
        let data = vec![read; 400 << 10].into_boxed_slice();
        media.cache(&[read; 32], &data, read.into()).unwrap();
    }
    let fits = media_replica(owner, b"fits".to_vec(), 1);
    assert_eq!(store_media_status(&s, &fits), Status::Ok);
    assert_eq!(media.cached(&[1; 32], 4).unwrap(), None);
    assert!(media.cached(&[3; 32], 4).unwrap().is_some());
    // Low disk space alone refuses storage.
    let s = media_service(|l| l.min_free_mb = 1 << 40);
    assert_eq!(store_media_status(&s, &fits), Status::Full);
}

#[test]
fn media_have_accepts_a_full_repair_window_and_no_more() {
    let s = media_service(|_| {});
    let status = |items| {
        let request = MediaRequest {
            request: Some(media_request::Request::Have(proto::MediaHave { items })),
        };
        Status::try_from(s.media(&peer("p"), request).unwrap().status).unwrap()
    };
    // Every hash of a window, whole and in all ten shards.
    let mut items = (0..u32::try_from(MAX_KEEP).unwrap())
        .flat_map(|i| {
            let hash = dyapp_identity::sha256(&i.to_be_bytes()).to_vec();
            std::iter::once(None)
                .chain((0..10).map(Some))
                .map(move |index| proto::MediaPart {
                    hash: hash.clone(),
                    index,
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(items.len(), crate::media_repair::MAX_ITEMS);
    assert_eq!(status(items.clone()), Status::Ok);
    items.push(items[0].clone());
    assert_eq!(status(items), Status::Invalid);
}
