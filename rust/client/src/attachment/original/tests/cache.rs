use super::*;
use crate::store::originals;
use image::{ImageFormat, RgbImage};
use std::io::Cursor;

const SEAL: [u8; 32] = [42; 32];

fn image(format: ImageFormat) -> Vec<u8> {
    if format == ImageFormat::Png {
        return include_bytes!("../validate/three-by-two.png").to_vec();
    }
    let mut bytes = Cursor::new(Vec::new());
    RgbImage::new(3, 2).write_to(&mut bytes, format).unwrap();
    bytes.into_inner()
}

fn selected(f: &Fixture) -> Snapshot {
    select(&f.store, &f.peer, FIRST, &f.peer, &ID).unwrap()
}

#[test]
fn jpeg_and_png_with_unspecified_dimensions_use_the_same_validated_cache_path() {
    for format in [ImageFormat::Jpeg, ImageFormat::Png] {
        let bytes = image(format);
        let f = Fixture::with_bytes(&bytes, b"other");
        f.store.set_kept_originals_mib(1).unwrap();
        let photo = selected(&f);
        assert!(photo.local(&f.store, &f.dir, &SEAL).unwrap().is_none());
        photo.finish(&f.store, &f.dir, &SEAL, &bytes).unwrap();
        assert_eq!(photo.local(&f.store, &f.dir, &SEAL).unwrap(), Some(bytes));
        assert!(originals::is_kept(&f.dir, &f.first.blob_sha256));
        assert!(f.store.due_outbound(i64::MAX).unwrap().is_empty());
    }
}

#[test]
fn oversized_cached_original_is_removed_without_decryption() {
    let f = Fixture::new();
    let path = f
        .dir
        .join("originals")
        .join(dyapp_identity::hex(&f.first.blob_sha256));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, vec![0; MAX_BLOB_BYTES + 1]).unwrap();
    assert_eq!(selected(&f).local(&f.store, &f.dir, &SEAL).unwrap(), None);
    assert!(!path.exists());
}

#[test]
fn authenticated_invalid_cached_originals_do_not_bypass_the_common_validator() {
    let f = Fixture::new();
    originals::keep(&f.dir, &SEAL, &f.first, b"first original").unwrap();
    assert!(matches!(
        selected(&f).local(&f.store, &f.dir, &SEAL),
        Err(LookupError::Corrupted)
    ));
}

#[test]
fn invalid_or_substituted_downloads_are_never_retained() {
    let bytes = image(ImageFormat::Png);
    let f = Fixture::with_bytes(&bytes, b"other");
    f.store.set_kept_originals_mib(1).unwrap();
    let photo = selected(&f);
    for invalid in [
        b"not an image".as_slice(),
        image(ImageFormat::Jpeg).as_slice(),
    ] {
        assert!(matches!(
            photo.finish(&f.store, &f.dir, &SEAL, invalid),
            Err(LookupError::Corrupted)
        ));
        assert!(!originals::is_kept(&f.dir, &f.first.blob_sha256));
    }
}

#[test]
fn blocked_or_cleared_messages_cannot_complete_an_already_selected_fetch() {
    let bytes = image(ImageFormat::Png);
    let mut f = Fixture::with_bytes(&bytes, b"other");
    f.store.set_kept_originals_mib(1).unwrap();
    let photo = selected(&f);
    f.store.set_blocked(&f.peer, true).unwrap();
    assert!(matches!(
        photo.finish(&f.store, &f.dir, &SEAL, &bytes),
        Err(LookupError::Blocked)
    ));
    assert!(matches!(
        photo.local(&f.store, &f.dir, &SEAL),
        Err(LookupError::Blocked)
    ));
    f.store.set_blocked(&f.peer, false).unwrap();
    f.store.clear_chat(FIRST).unwrap();
    assert!(matches!(
        photo.finish(&f.store, &f.dir, &SEAL, &bytes),
        Err(LookupError::NotFound)
    ));
    assert!(!originals::is_kept(&f.dir, &f.first.blob_sha256));
}

#[test]
fn disabled_retention_and_cache_write_failure_do_not_fail_a_valid_download() {
    let bytes = image(ImageFormat::Jpeg);
    let f = Fixture::with_bytes(&bytes, b"other");
    let photo = selected(&f);
    f.store.set_kept_originals_mib(0).unwrap();
    photo.finish(&f.store, &f.dir, &SEAL, &bytes).unwrap();
    assert!(!originals::is_kept(&f.dir, &f.first.blob_sha256));
    f.store.set_kept_originals_mib(1).unwrap();
    // A file at the directory path deterministically refuses cache creation.
    std::fs::write(f.dir.join("originals"), b"reserved fixture file").unwrap();
    photo.finish(&f.store, &f.dir, &SEAL, &bytes).unwrap();
    assert!(photo.local(&f.store, &f.dir, &SEAL).unwrap().is_none());
}

#[test]
fn selected_original_cannot_be_completed_under_another_identity() {
    let bytes = image(ImageFormat::Png);
    let f = Fixture::with_bytes(&bytes, b"other");
    let other = Fixture::with_bytes(&bytes, b"other");
    assert!(matches!(
        selected(&f).finish(&other.store, &other.dir, &SEAL, &bytes),
        Err(LookupError::NotFound)
    ));
    assert!(!originals::is_kept(&other.dir, &f.first.blob_sha256));
}
