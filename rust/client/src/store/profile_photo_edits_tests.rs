use super::{revision, PhotoEdit};
use crate::store::{tests::temp_db_path, Error, Store};
use dyapp_identity::Identity;

fn photo(store: &mut Store, original: u8) -> dyapp_profile::Profile {
    // Equal previews deliberately cannot identify these different originals.
    store
        .add_profile_photo(&[0xFF, 0xD8, 0xFF, original], &[0xFF, 0xD8, 0xFF, 9])
        .unwrap()
}

#[test]
fn stale_edits_refuse_even_when_reordered_originals_have_identical_previews() {
    let (path, owner) = (temp_db_path(), Identity::generate());
    let mut displayed = Store::open(&path, &owner).unwrap();
    let mut other = Store::open(&path, &owner).unwrap();
    photo(&mut displayed, 1);
    let before = photo(&mut displayed, 2);
    let token = revision(&before);
    let after = other.move_profile_photo(1, 0).unwrap();
    assert_eq!(before.photos[0].thumbnail, after.photos[0].thumbnail);
    assert_ne!(token, revision(&after));
    for edit in [PhotoEdit::Remove(0), PhotoEdit::Move { from: 0, to: 1 }] {
        assert!(matches!(
            displayed.edit_profile_photos(&token, edit),
            Err(Error::PhotoChanged)
        ));
        assert_eq!(displayed.profile_draft().unwrap(), Some(after.clone()));
    }
    let fresh = displayed
        .edit_profile_photos(&revision(&after), PhotoEdit::Remove(0))
        .unwrap();
    assert_eq!(fresh.photos, [before.photos[0].clone()]);
    assert!(matches!(
        other.edit_profile_photos(&revision(&after), PhotoEdit::Remove(1)),
        Err(Error::PhotoChanged)
    ));
}

#[test]
fn unrelated_fields_and_equivalent_duplicates_do_not_invalidate_the_snapshot() {
    let (path, owner) = (temp_db_path(), Identity::generate());
    let mut displayed = Store::open(&path, &owner).unwrap();
    let mut other = Store::open(&path, &owner).unwrap();
    photo(&mut displayed, 1);
    let before = photo(&mut displayed, 1);
    let token = revision(&before);
    other
        .update_profile_draft(|draft| {
            draft.version = 7;
            draft.pow_bits = 4;
            Ok(())
        })
        .unwrap();
    let moved = displayed
        .edit_profile_photos(&token, PhotoEdit::Move { from: 1, to: 0 })
        .unwrap();
    assert_eq!(revision(&moved), token);
    let removed = displayed
        .edit_profile_photos(&token, PhotoEdit::Remove(1))
        .unwrap();
    assert_eq!(removed.photos, [before.photos[0].clone()]);
    assert_eq!((removed.version, removed.pow_bits), (7, 4));
    assert!(matches!(
        displayed.edit_profile_photos(&[], PhotoEdit::Remove(0)),
        Err(Error::PhotoChanged)
    ));
    assert_eq!(other.profile_draft().unwrap(), Some(removed));
}
