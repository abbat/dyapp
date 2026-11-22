use super::*;
use dyapp_client::photo;
use prost::Message as _;
use std::sync::atomic::Ordering;

fn remote_photo(f: &Fixture, bytes: &[u8]) -> Vec<u8> {
    let hash = dyapp_identity::sha256(bytes).to_vec();
    let mut record = f.server.record.lock().unwrap();
    let mut remote = Profile::decode(record.payload.as_slice()).unwrap();
    remote.photos = vec![dyapp_profile::Photo {
        thumbnail: hash.clone(),
        full: vec![vec![9; 32]],
    }];
    *record = remote.sign(&f.slot.key.identity);
    hash
}

fn thumbnail() -> Vec<u8> {
    photo::make_preview(&[10, 30, 80, 255, 90, 60, 20, 255], 2, 1, photo::MAX_BYTES).unwrap()
}

#[test]
fn unavailable_remote_thumbnail_can_retry_without_persistence_or_selecting_a_source() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let bytes = thumbnail();
    let hash = remote_photo(&f, &bytes);
    let review = f.worker.prepare().unwrap();
    let before = f.store().profile_draft().unwrap();
    assert!(matches!(
        review.photo_thumbnail(LinkReviewSource::Remote, 0),
        Err(LinkReviewError::Failed { .. })
    ));
    f.server
        .media
        .blobs
        .lock()
        .unwrap()
        .insert(hash.clone(), bytes.clone());
    let decoded = review.photo_thumbnail(LinkReviewSource::Remote, 0).unwrap();
    assert_eq!(decoded.pixels, photo::check_preview(&bytes).unwrap().pixels);
    assert_eq!(f.server.media.reads.load(Ordering::SeqCst), 2);
    assert_eq!(f.server.profile_writes.load(Ordering::SeqCst), 0);
    assert_eq!(f.store().profile_photo(&hash).unwrap(), None);
    assert_eq!(f.store().profile_draft().unwrap(), before);
    assert_eq!(
        f.store().link_pending().unwrap(),
        Some(b"original link".to_vec())
    );
}

#[test]
fn wrong_hash_or_matching_hash_invalid_jpeg_never_reaches_the_platform_decoder() {
    let valid = thumbnail();
    for (offered, returned) in [
        (&valid[..], &b"wrong hash"[..]),
        (&b"not a JPEG"[..], &b"not a JPEG"[..]),
    ] {
        let f = Fixture::new(MobileSlotLayout::Android);
        let hash = remote_photo(&f, offered);
        f.server
            .media
            .blobs
            .lock()
            .unwrap()
            .insert(hash, returned.to_vec());
        let review = f.worker.prepare().unwrap();
        assert!(matches!(
            review.photo_thumbnail(LinkReviewSource::Remote, 0),
            Err(LinkReviewError::Failed { .. })
        ));
        assert_eq!(f.server.media.reads.load(Ordering::SeqCst), 1);
        assert_eq!(review.snapshot().remote.photos.len(), 1);
    }
}

#[test]
fn actual_media_response_rechecks_the_original_network_before_returning_pixels() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let bytes = thumbnail();
    let hash = remote_photo(&f, &bytes);
    f.server.media.blobs.lock().unwrap().insert(hash, bytes);
    let review = f.worker.prepare().unwrap();
    let directory = f.directory.clone();
    let mut replacement = f.server.network.clone();
    replacement.name.push_str("-replacement");
    *f.server.media.on_get.lock().unwrap() = Some(Box::new(move || {
        dyapp_identity::network::save(
            &directory,
            dyapp_identity::network::Settings::Active,
            &replacement,
        )
        .unwrap();
    }));
    assert!(matches!(
        review.photo_thumbnail(LinkReviewSource::Remote, 0),
        Err(LinkReviewError::ReviewRequired)
    ));
    assert_eq!(f.server.media.reads.load(Ordering::SeqCst), 1);
}
