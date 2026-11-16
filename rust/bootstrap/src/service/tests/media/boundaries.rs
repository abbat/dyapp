//! Media admission boundary tests.
use super::*;

#[test]
fn legacy_keep_admits_exact_version_hash_and_clock_boundaries() {
    let mut s = media_service(|_| {});
    let manual = crate::clock::Manual::at(1_800_000_000);
    s.clock = manual.clock();
    let now = s.clock.timestamp().unsigned_abs();
    let check = |keep: proto::MediaKeep| {
        let owner = Identity::generate();
        let record = owner.sign(Domain::MediaKeep, keep.encode_to_vec());
        Status::try_from(
            s.media(
                &peer("legacy-boundary"),
                MediaRequest {
                    request: Some(media_request::Request::Keep(record)),
                },
            )
            .unwrap()
            .status,
        )
        .unwrap()
    };
    let keep = |version, hashes, time| proto::MediaKeep {
        version,
        hashes,
        time,
    };
    let hash = || media_hashes(b"boundary", 1);
    for (candidate, expected) in [
        (keep(0, hash(), now), Status::Invalid),
        (keep(1, hash(), now), Status::Ok),
        (keep(i64::MAX.cast_unsigned(), hash(), now), Status::Ok),
        (
            keep(i64::MAX.cast_unsigned() + 1, hash(), now),
            Status::Invalid,
        ),
        (
            keep(1, media_hashes(b"boundary", MAX_KEEP), now),
            Status::Ok,
        ),
        (
            keep(1, media_hashes(b"boundary", MAX_KEEP + 1), now),
            Status::Invalid,
        ),
        (keep(1, hash(), now + CLOCK_SKEW), Status::Ok),
        (keep(1, hash(), now + CLOCK_SKEW + 1), Status::Invalid),
    ] {
        assert_eq!(check(candidate), expected);
    }
}

#[test]
fn conditional_keep_admits_exact_clock_boundaries() {
    let mut s = media_service(|_| {});
    let manual = crate::clock::Manual::at(1_800_000_000);
    s.clock = manual.clock();
    let now = s.clock.timestamp().unsigned_abs();
    for (time, expected) in [
        (now + CLOCK_SKEW, Status::Ok),
        (now + CLOCK_SKEW + 1, Status::Invalid),
        (now - CLOCK_SKEW, Status::Ok),
        (now - CLOCK_SKEW - 1, Status::Invalid),
    ] {
        let owner = Identity::generate();
        let caller = Peer {
            id: libp2p::PeerId::random().to_string(),
            group: "g".into(),
            coarse: "g".into(),
        };
        let keep = proto::MediaKeepIntent {
            version: 1,
            hashes: media_hashes(b"conditional", 1),
            time,
            expected: Some(proto::MediaKeepCondition::default()),
            intent_hash: vec![0; 32],
            requester: caller.id.clone(),
            pending: false,
        };
        let record = owner.sign(Domain::MediaConditionalKeep, keep.encode_to_vec());
        let response = s
            .media(
                &caller,
                MediaRequest {
                    request: Some(media_request::Request::ConditionalKeep(record)),
                },
            )
            .unwrap();
        assert_eq!(
            Status::try_from(response.status).unwrap(),
            expected,
            "keep time {time}, node time {now}"
        );
    }
}

#[test]
fn attachment_admits_the_exact_future_clock_boundary() {
    let mut s = media_service(|_| {});
    let manual = crate::clock::Manual::at(1_800_000_000);
    s.clock = manual.clock();
    let now = s.clock.timestamp().unsigned_abs();
    for (created, expected) in [
        (now + CLOCK_SKEW, Status::Ok),
        (now + CLOCK_SKEW + 1, Status::Invalid),
    ] {
        let owner = Identity::generate();
        let data = b"attachment boundary".to_vec();
        let attach = proto::MediaAttach {
            release: vec![9; 32],
            hashes: vec![dyapp_identity::sha256(&data).to_vec()],
            created,
        };
        let replica = proto::MediaReplicaPut {
            data,
            manifest: None,
            authorization: Some(proto::media_replica_put::Authorization::Attach(
                owner.sign(Domain::MediaAttach, attach.encode_to_vec()),
            )),
        };
        assert_eq!(
            Status::try_from(s.store_media(&replica).unwrap().status).unwrap(),
            expected
        );
    }
}
