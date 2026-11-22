use super::*;
use dyapp_client::photo;

fn cached_photo(f: &Fixture) -> Vec<u8> {
    let bytes =
        photo::make_preview(&[10, 30, 80, 255, 90, 60, 20, 255], 2, 1, photo::MAX_BYTES).unwrap();
    let store = f.store();
    let hash = store.save_profile_blob(&bytes).unwrap();
    let mut draft = store.profile_draft().unwrap().unwrap();
    draft.photos = vec![dyapp_profile::Photo {
        thumbnail: hash,
        full: vec![vec![7; 32], vec![8; 32]],
    }];
    store.save_profile_draft(&draft).unwrap();
    bytes
}

#[test]
fn original_slot_thumbnail_works_offline_after_selection_changes_and_never_saves() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let mut f = Fixture::new(layout);
        let bytes = cached_photo(&f);
        let review = f.worker.prepare().unwrap();
        let snapshot = review.snapshot();
        let other = f.registry.create().unwrap();
        f.server.shutdown();
        let decoded = review.photo_thumbnail(LinkReviewSource::Local, 0).unwrap();
        let expected = photo::check_preview(&bytes).unwrap();
        assert_eq!(
            (decoded.width, decoded.height, decoded.pixels),
            (expected.width, expected.height, expected.pixels)
        );
        assert_eq!(review.snapshot().local, snapshot.local);
        assert_eq!(f.store().profile_draft().unwrap().unwrap().photos.len(), 1);
        assert_eq!(
            f.store().link_pending().unwrap(),
            Some(b"original link".to_vec())
        );
        assert!(f.store().profile_hidden().unwrap());
        assert!(!PathBuf::from(other.slot.directory)
            .join("client.db")
            .exists());
    }
}

#[test]
fn cached_pixels_do_not_bypass_a_changed_original_network_or_draft() {
    let f = Fixture::new(MobileSlotLayout::Android);
    cached_photo(&f);
    let review = f.worker.prepare().unwrap();
    let mut draft = f.store().profile_draft().unwrap().unwrap();
    draft.about = "a later edit".into();
    f.store().save_profile_draft(&draft).unwrap();
    assert!(matches!(
        review.photo_thumbnail(LinkReviewSource::Local, 0),
        Err(LinkReviewError::ReviewRequired)
    ));
    let fresh = f.worker.prepare().unwrap();
    let mut changed = f.server.network.clone();
    changed.name.push_str("-changed");
    dyapp_identity::network::save(
        &f.directory,
        dyapp_identity::network::Settings::Active,
        &changed,
    )
    .unwrap();
    assert!(matches!(
        fresh.photo_thumbnail(LinkReviewSource::Local, 0),
        Err(LinkReviewError::ReviewRequired)
    ));
}

#[test]
fn photo_indices_are_only_resolved_from_the_exact_selected_frozen_source() {
    let f = Fixture::new(MobileSlotLayout::Android);
    cached_photo(&f);
    let review = f.worker.prepare().unwrap();
    assert!(review.photo_thumbnail(LinkReviewSource::Local, 0).is_ok());
    assert!(matches!(
        review.photo_thumbnail(LinkReviewSource::Local, 1),
        Err(LinkReviewError::Failed { .. })
    ));
    assert!(matches!(
        review.photo_thumbnail(LinkReviewSource::Remote, 0),
        Err(LinkReviewError::Failed { .. })
    ));
    assert!(matches!(
        review.photo_thumbnail(LinkReviewSource::Local, u32::MAX),
        Err(LinkReviewError::Failed { .. })
    ));
}
