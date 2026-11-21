use super::*;
use crate::store::tests::temp_db_path;
use dyapp_identity::Identity;
use dyapp_profile::{Photo, Profile};

fn thumbnail(value: u8) -> Vec<u8> {
    photo::make_preview(
        &[value, 30, 80, 255, value, 60, 90, 255],
        2,
        1,
        photo::MAX_BYTES,
    )
    .unwrap()
}

fn reference(bytes: &[u8]) -> Photo {
    Photo {
        thumbnail: dyapp_identity::sha256(bytes).to_vec(),
        full: vec![vec![5; 32], vec![6; 32]],
    }
}

fn frozen(local: Profile, remote: Profile) -> (AdoptionReview, Store) {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store.save_profile_draft(&local).unwrap();
    store
        .set_link_pending(b"original pending comparison")
        .unwrap();
    let context = store
        .prepare_profile_adoption_context(&identity)
        .unwrap()
        .unwrap();
    (AdoptionReview { context, remote }, store)
}

#[test]
fn cached_sources_keep_frozen_order_and_do_not_write_or_select_fields() {
    let first = thumbnail(10);
    let second = thumbnail(200);
    let local = Profile {
        photos: vec![reference(&first), reference(&second)],
        ..Default::default()
    };
    let remote = Profile {
        photos: vec![reference(&second)],
        ..Default::default()
    };
    let (review, store) = frozen(local.clone(), remote.clone());
    store.save_profile_blob(&first).unwrap();
    store.save_profile_blob(&second).unwrap();
    let changed = Profile {
        name: "later unrelated draft".into(),
        ..Default::default()
    };
    store.save_profile_draft(&changed).unwrap();
    for (source, index, bytes) in [
        (AdoptionSource::Local, 0, &first),
        (AdoptionSource::Local, 1, &second),
        (AdoptionSource::Remote, 0, &second),
    ] {
        assert_eq!(
            review.cached_thumbnail(&store, source, index).unwrap(),
            Some(photo::check_preview(bytes).unwrap())
        );
    }
    assert_eq!(review.local_profile(), &local);
    assert_eq!(review.remote_profile(), &remote);
    assert_eq!(store.profile_draft().unwrap(), Some(changed));
    assert_eq!(
        store.link_pending().unwrap(),
        Some(b"original pending comparison".to_vec())
    );
}

#[tokio::test]
async fn unavailable_or_invalid_references_never_become_an_empty_photo_group() {
    let missing = thumbnail(40);
    let local = Profile {
        photos: vec![reference(&missing)],
        ..Default::default()
    };
    let remote = Profile {
        photos: vec![Photo {
            thumbnail: vec![1; 31],
            full: vec![],
        }],
        ..Default::default()
    };
    let (review, store) = frozen(local.clone(), remote.clone());
    assert_eq!(
        review
            .cached_thumbnail(&store, AdoptionSource::Local, 0)
            .unwrap(),
        None
    );
    assert!(matches!(
        review.cached_thumbnail(&store, AdoptionSource::Local, 1),
        Err(AdoptionPhotoError::Index)
    ));
    assert!(matches!(
        review.cached_thumbnail(&store, AdoptionSource::Remote, 0),
        Err(AdoptionPhotoError::Reference)
    ));
    assert!(matches!(
        review
            .fetch_thumbnail(&mut [], AdoptionSource::Local, 0)
            .await,
        Err(AdoptionPhotoError::Unavailable)
    ));
    assert_eq!(review.local_profile(), &local);
    assert_eq!(review.remote_profile(), &remote);
    assert_eq!(store.profile_draft().unwrap(), Some(local));
}

#[test]
fn cached_or_downloaded_bytes_need_both_the_exact_hash_and_bounded_decode() {
    let bytes = thumbnail(20);
    assert!(decode_thumbnail(&[0; 32], &bytes).is_none());
    assert!(decode_thumbnail(&dyapp_identity::sha256(&bytes), &bytes).is_some());
    let invalid = b"not a JPEG";
    assert!(decode_thumbnail(&dyapp_identity::sha256(invalid), invalid).is_none());
    let mut large = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut large)
        .encode_image(&image::RgbImage::new(photo::MAX_SIDE + 1, 1))
        .unwrap();
    assert!(decode_thumbnail(&dyapp_identity::sha256(&large), &large).is_none());
}
