use super::*;
use dyapp_identity::Identity;

#[test]
fn snapshot_edits_keep_stale_refusal_typed_at_the_native_boundary() {
    let owner = Identity::generate();
    let path = std::env::temp_dir().join(format!("photo-edits-{}", owner.peer_id()));
    std::fs::create_dir_all(&path).unwrap();
    let dir = path.to_str().unwrap().to_owned();
    let key = Arc::new(IdentityKey { identity: owner });
    let mut other = Store::open(&path.join("client.db"), &key.identity).unwrap();
    for seed in [1, 2] {
        other
            .add_profile_photo(&[0xFF, 0xD8, 0xFF, seed], &[0xFF, 0xD8, 0xFF, 9])
            .unwrap();
    }
    let before = profile_photo_snapshot(dir.clone(), key.clone()).unwrap();
    other.move_profile_photo(1, 0).unwrap();
    let refusal = edit_profile_photos(
        dir.clone(),
        key.clone(),
        before.revision,
        ProfilePhotoEdit::Remove { index: 0 },
    );
    assert!(matches!(refusal, Err(ProfilePhotoEditError::Changed)));
    let fresh = profile_photo_snapshot(dir.clone(), key.clone()).unwrap();
    let changed = edit_profile_photos(
        dir.clone(),
        key.clone(),
        fresh.revision,
        ProfilePhotoEdit::Move { from: 1, to: 0 },
    )
    .unwrap();
    assert_eq!(changed.thumbnails.len(), 2);
    let removed = edit_profile_photos(
        dir,
        key,
        changed.revision,
        ProfilePhotoEdit::Remove { index: 1 },
    )
    .unwrap();
    assert_eq!(removed.thumbnails.len(), 1);
}
