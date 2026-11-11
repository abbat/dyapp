//! One-shot request-boundary transitions exercise real SQLite ownership without timing guesses.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn current(fixture: &Fixture) -> proto::MediaKeepIntent {
    let record = fixture.media.keep_of(&fixture.owner).unwrap().unwrap();
    proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap()
}

fn advance(
    fixture: &Fixture,
    identity: &Identity,
    change: impl FnOnce(&mut proto::MediaKeepIntent),
) {
    let snapshot = fixture.media.keep_snapshot(&fixture.owner).unwrap();
    let record = snapshot.record.as_ref().unwrap();
    let mut intent = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
    intent.version = snapshot.version + 1;
    intent.time += 1;
    intent.expected = Some(proto::MediaKeepCondition {
        owner_present: true,
        version: snapshot.version,
        record_hash: sha256(&record.encode_to_vec()).to_vec(),
    });
    if intent.requester.is_empty() {
        intent.requester = Keypair::ed25519_from_bytes([29; 32])
            .unwrap()
            .public()
            .to_peer_id()
            .to_base58();
    }
    change(&mut intent);
    let signed = identity.sign(Domain::MediaConditionalKeep, intent.encode_to_vec());
    let keep = proto::MediaKeep::decode(signed.payload.as_slice()).unwrap();
    assert!(matches!(
        fixture
            .media
            .replace_keep(&fixture.owner, &snapshot, &keep, &signed)
            .unwrap(),
        KeepUpdate::Applied(_)
    ));
}

#[tokio::test]
async fn concurrent_refresh_cannot_strand_a_successfully_published_target() {
    for finalized in [false, true] {
        let (identity, store, original) = local();
        let mut fixture = duplicate(&identity, &store, &original, 6);
        let media = Arc::clone(&fixture.media);
        let called = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&called);
        let signer = Identity::from_secret(&identity.secret());
        fixture.before_finalize = Some(Box::new(move |fixture| {
            advance(fixture, &signer, |intent| intent.pending = !finalized);
            observed.fetch_add(1, Ordering::SeqCst);
        }));
        let mut node = connect(fixture).await;
        let published = publish::publish(&store, &mut node, &identity, &original)
            .await
            .unwrap();
        assert_eq!(called.load(Ordering::SeqCst), 1);
        let record = media.keep_of(&identity.peer_id()).unwrap().unwrap();
        let intent = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
        assert!(!intent.pending);
        assert_eq!(
            intent.intent_hash,
            sha256(&published.sign(&identity).encode_to_vec())
        );
        assert_eq!(store.profile_draft().unwrap(), Some(published));
    }
}

async fn foreign_stage(withdrawal: bool) {
    let (identity, store, original) = local();
    let mut fixture = duplicate(&identity, &store, &original, 6);
    let media = Arc::clone(&fixture.media);
    let (mut foreign, bytes) = replacement(&original);
    foreign.version = 8;
    let foreign_record = foreign.sign(&identity);
    let target = sha256(&foreign_record.encode_to_vec());
    let expected_bytes = bytes.clone();
    let signer = Identity::from_secret(&identity.secret());
    fixture.before_finalize = Some(Box::new(move |fixture| {
        advance(fixture, &signer, |intent| {
            intent.hashes.push(sha256(&bytes).to_vec());
            intent.intent_hash = target.to_vec();
            intent.pending = true;
        });
        assert_eq!(
            fixture
                .media
                .put(&fixture.owner, &bytes, 1_000_000)
                .unwrap(),
            Put::Stored
        );
        fixture.held = foreign_record;
    }));
    let mut node = connect(fixture).await;
    if withdrawal {
        publish::withdraw(&store, &mut node, &identity, &original)
            .await
            .unwrap();
    } else {
        publish::publish(&store, &mut node, &identity, &original)
            .await
            .unwrap();
    }
    assert_present(&media, &sha256(&expected_bytes), &expected_bytes);
    let record = media.keep_of(&identity.peer_id()).unwrap().unwrap();
    let intent = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
    assert!(intent.pending);
    assert_eq!(intent.intent_hash, target);
}

#[tokio::test]
async fn stale_publish_finalize_preserves_a_newer_foreign_stage() {
    Box::pin(foreign_stage(false)).await;
}

#[tokio::test]
async fn stale_withdraw_finalize_preserves_a_newer_foreign_stage() {
    Box::pin(foreign_stage(true)).await;
}

#[tokio::test]
async fn lost_staged_ownership_is_an_explicit_failure_and_can_be_retried() {
    let (identity, store, original) = local();
    let mut fixture = duplicate(&identity, &store, &original, 6);
    let media = Arc::clone(&fixture.media);
    fixture.before_finalize = Some(Box::new(|fixture| {
        let cutoff = chrono::Utc::now().timestamp() + 1;
        assert_eq!(fixture.media.expire_owners(cutoff).unwrap(), 1);
    }));
    let mut node = connect(fixture).await;
    let first = publish::publish(&store, &mut node, &identity, &original).await;
    assert!(matches!(
        first,
        Err(publish::Failure::Node(Error::Status(Status::Stale)))
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(original.clone()));
    let published = publish::publish(&store, &mut node, &identity, &original)
        .await
        .unwrap();
    assert_eq!(published.version, 6);
    for hash in hashes(&published) {
        assert!(media.get(&hash).unwrap().is_some());
    }
}

#[tokio::test]
async fn refresh_uses_the_exact_signed_profile_with_unknown_wire_fields() {
    let (identity, store, original) = local();
    let mut fixture = duplicate(&identity, &store, &original, 6);
    let mut payload = original.encode_to_vec();
    // Unknown field 127, varint value 1, is signed and survives verified raw readback.
    payload.extend_from_slice(&[0xf8, 0x07, 1]);
    fixture.held = identity.sign(Domain::Profile, payload);
    let target = sha256(&fixture.held.encode_to_vec());
    assert_ne!(target, sha256(&original.sign(&identity).encode_to_vec()));
    advance(&fixture, &identity, |intent| {
        intent.intent_hash = target.to_vec();
        intent.pending = true;
    });
    assert!(current(&fixture).pending);
    let media = Arc::clone(&fixture.media);
    let mut node = connect(fixture).await;
    publish::refresh_photos(&store, &mut node, &identity, 100)
        .await
        .unwrap();
    let record = media.keep_of(&identity.peer_id()).unwrap().unwrap();
    let intent = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
    assert_eq!(intent.intent_hash, target);
    assert!(!intent.pending);
}

#[tokio::test]
async fn exhausted_media_revision_preserves_the_draft_and_existing_ownership() {
    let (identity, store, original) = local();
    let fixture = duplicate(&identity, &store, &original, i64::MAX.cast_unsigned());
    let media = Arc::clone(&fixture.media);
    let before = media.keep_snapshot(&identity.peer_id()).unwrap();
    let mut node = connect(fixture).await;
    let result = publish::publish(&store, &mut node, &identity, &original).await;
    assert!(matches!(
        result,
        Err(publish::Failure::Node(Error::Input(_)))
    ));
    assert_eq!(media.keep_snapshot(&identity.peer_id()).unwrap(), before);
    assert_eq!(store.profile_draft().unwrap(), Some(original));
}

fn reopened(identity: &Identity, profile: &Profile, dir: &std::path::Path) -> Fixture {
    Fixture {
        owner: identity.peer_id(),
        media: Arc::new(MediaStore::open(dir).unwrap()),
        held: profile.sign(identity),
        replacement: None,
        keeps: Arc::new(Mutex::new(Vec::new())),
        refuse_publish_once: false,
        before_finalize: None,
        quota: 1_000_000,
        reject_conditional: None,
    }
}

#[tokio::test]
async fn quota_failure_retains_live_references_and_the_changed_draft() {
    let (identity, store, original) = local();
    let mut fixture = duplicate(&identity, &store, &original, 6);
    fixture.quota = 0;
    let media = Arc::clone(&fixture.media);
    let (mut changed, bytes) = replacement(&original);
    changed.version = original.version;
    store.save_profile_blob(&bytes).unwrap();
    store.save_profile_draft(&changed).unwrap();
    let mut node = connect(fixture).await;
    let result = publish::publish(&store, &mut node, &identity, &changed).await;
    assert!(matches!(
        result,
        Err(publish::Failure::Node(Error::Status(Status::Full)))
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(changed));
    for hash in hashes(&original) {
        assert!(media.get(&hash).unwrap().is_some());
    }
    assert!(media.get(&sha256(&bytes)).unwrap().is_none());
    let record = media.keep_of(&identity.peer_id()).unwrap().unwrap();
    let intent = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
    assert!(intent.pending);
    assert!(hashes(&original)
        .iter()
        .all(|hash| intent.hashes.contains(hash)));
}

#[tokio::test]
async fn unsupported_conditional_protocol_preserves_existing_photos_and_draft() {
    let (identity, store, original) = local();
    let mut fixture = duplicate(&identity, &store, &original, 6);
    fixture.reject_conditional = Some(Status::Unsupported);
    let media = Arc::clone(&fixture.media);
    let before = media.keep_snapshot(&identity.peer_id()).unwrap();
    let mut node = connect(fixture).await;
    let result = publish::publish(&store, &mut node, &identity, &original).await;
    assert!(matches!(
        result,
        Err(publish::Failure::Node(Error::Status(Status::Unsupported)))
    ));
    assert_eq!(media.keep_snapshot(&identity.peer_id()).unwrap(), before);
    assert_eq!(store.profile_draft().unwrap(), Some(original));
}

#[tokio::test]
async fn photo_free_publication_can_use_a_store_without_conditional_media() {
    let (identity, store, original) = local();
    let mut fixture = duplicate(&identity, &store, &original, 6);
    fixture.reject_conditional = Some(Status::Unsupported);
    let media = Arc::clone(&fixture.media);
    let before = media.keep_snapshot(&identity.peer_id()).unwrap();
    let mut changed = original.clone();
    changed.photos.clear();
    store.save_profile_draft(&changed).unwrap();
    let mut node = connect(fixture).await;
    let published = publish::publish(&store, &mut node, &identity, &changed)
        .await
        .unwrap();
    assert_eq!(published.photos, Vec::<dyapp_profile::Photo>::new());
    assert_eq!(published.version, 6);
    assert_eq!(media.keep_snapshot(&identity.peer_id()).unwrap(), before);
}

#[tokio::test]
async fn reopened_storage_recovers_the_persisted_pending_stage() {
    let (identity, store, original) = local();
    let dir = path();
    let mut fixture = reopened(&identity, &original, &dir);
    fixture.refuse_publish_once = true;
    let media = Arc::clone(&fixture.media);
    let mut node = connect(fixture).await;
    let result = publish::publish(&store, &mut node, &identity, &original).await;
    assert!(matches!(
        result,
        Err(publish::Failure::Node(Error::Status(Status::Refused)))
    ));
    let pending = media.keep_snapshot(&identity.peer_id()).unwrap();
    drop(node);
    let fresh = reopened(&identity, &original, &dir);
    assert_eq!(
        fresh.media.keep_snapshot(&identity.peer_id()).unwrap(),
        pending
    );
    let loaded = current(&fresh);
    assert!(loaded.pending);
    let fresh_media = Arc::clone(&fresh.media);
    let mut node = connect(fresh).await;
    let published = publish::publish(&store, &mut node, &identity, &original)
        .await
        .unwrap();
    let record = fresh_media.keep_of(&identity.peer_id()).unwrap().unwrap();
    let intent = proto::MediaKeepIntent::decode(record.payload.as_slice()).unwrap();
    assert!(!intent.pending);
    assert!(intent.version > pending.version);
    assert_eq!(published.version, 6);
}
