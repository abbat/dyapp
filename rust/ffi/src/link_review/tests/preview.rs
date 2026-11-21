use super::*;

#[test]
fn selection_preview_uses_frozen_values_without_saving_or_reading_the_network() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap();
    let snapshot = review.snapshot();
    let stored = f.store().profile_draft().unwrap();
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = Arc::clone(&reads);
    *f.server.on_read.lock().unwrap() = Some(Box::new(move || {
        observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    assert!(matches!(
        review.preview(Vec::new()),
        Err(LinkReviewError::Selection { .. })
    ));
    let mut duplicate = Fixture::choices();
    duplicate.extend(Fixture::choices());
    assert!(matches!(
        review.preview(duplicate),
        Err(LinkReviewError::Selection { .. })
    ));
    assert!(matches!(
        review.preview(vec![LinkReviewChoice {
            field: LinkReviewField::Photos,
            source: LinkReviewSource::Remote,
        }]),
        Err(LinkReviewError::Selection { .. })
    ));
    let local = review.preview(Fixture::choices()).unwrap();
    assert_eq!(local.fields.name, "local");
    assert_eq!(local.version, snapshot.remote.version);
    assert_eq!(
        review
            .preview(vec![LinkReviewChoice {
                field: LinkReviewField::Name,
                source: LinkReviewSource::Remote,
            }])
            .unwrap(),
        snapshot.remote
    );
    assert_eq!(review.snapshot().local, snapshot.local);
    assert_eq!(f.store().profile_draft().unwrap(), stored);
    assert_eq!(
        f.store().link_pending().unwrap(),
        Some(b"original link".to_vec())
    );
    assert!(f.store().profile_hidden().unwrap());
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 0);
}
