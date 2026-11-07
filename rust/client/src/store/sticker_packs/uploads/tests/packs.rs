use super::*;

fn pack_candidate() -> Attachment {
    let pack = crate::store::sticker_packs::tests::pack("Cats");
    let (mut descriptor, _) = attachment::seal(&pack.encode()).unwrap();
    descriptor.mime = "application/octet-stream".into();
    descriptor
}
fn as_pack(mut descriptor: Attachment) -> Attachment {
    descriptor.mime = "application/octet-stream".into();
    descriptor.width = 0;
    descriptor.height = 0;
    descriptor
}

#[test]
fn sealed_pack_limit_and_single_sticker_budget_are_independent() {
    let descriptor = as_pack(candidate(99, 6289920));
    assert_eq!(encode(&descriptor, Kind::Pack).unwrap().1, 31457280);
    let mut invalid = descriptor.clone();
    invalid.size += 1;
    assert!(encode(&invalid, Kind::Pack).is_err());
    invalid = descriptor.clone();
    invalid.width = 1;
    assert!(encode(&invalid, Kind::Pack).is_err());
    let (mut store, _, _) = database();
    for index in 0..12 {
        upload(
            store
                .plan_sticker_upload(&candidate(index, 262144), 100)
                .unwrap(),
        );
    }
    upload(
        store
            .plan_sticker_upload(&candidate(20, 208883), 100)
            .unwrap(),
    );
    upload(store.plan_sticker_pack_upload(&descriptor, 100).unwrap());
    assert_eq!(used(&store), 16777215);
    assert!(matches!(
        store.plan_sticker_upload(&candidate(21, 1), 100).unwrap(),
        UploadPlan::Fallback
    ));
}

#[test]
fn pack_descriptor_reuses_after_restart_and_refresh_keeps_immutable_blob() {
    let (mut store, owner, path) = database();
    let first = pack_candidate();
    let newer = pack_candidate();
    let reserved = upload(store.plan_sticker_pack_upload(&first, 100).unwrap());
    assert!(upload(store.plan_sticker_pack_upload(&newer, 101).unwrap()) == reserved);
    store.finish_sticker_pack_upload(&reserved, 102).unwrap();
    drop(store);
    let mut store = Store::open(&path, &owner).unwrap();
    assert!(
        matches!(store.plan_sticker_pack_upload(&newer, 103).unwrap(),
        UploadPlan::Reuse(ref value) if *value == reserved.attachment)
    );
    let now = 103 + 6 * 86400;
    let refresh = upload(store.plan_sticker_pack_upload(&newer, now).unwrap());
    assert!(
        refresh.attachment == reserved.attachment && refresh.reservation != reserved.reservation
    );
    assert!(store.finish_sticker_pack_upload(&reserved, now).is_err());
    assert!(store.finish_sticker_upload(&refresh, now).is_err());
    store.finish_sticker_pack_upload(&refresh, now).unwrap();
    let expired = upload(
        store
            .plan_sticker_pack_upload(&newer, now + 8 * 86400)
            .unwrap(),
    );
    assert!(expired.attachment == newer);
}

#[test]
fn equal_plaintext_hash_cannot_cross_upload_kind_or_confirmation() {
    // Metadata fixtures deliberately share bytes; real validated WebP/pack formats are distinct.
    let image = candidate(1, 100);
    let pack = as_pack(candidate(1, 100));
    assert_eq!(image.plaintext_sha256, pack.plaintext_sha256);
    let (mut store, _, _) = database();
    let image_pending = upload(store.plan_sticker_upload(&image, 100).unwrap());
    let pack_pending = upload(store.plan_sticker_pack_upload(&pack, 100).unwrap());
    store
        .finish_sticker_pack_upload(&pack_pending, 100)
        .unwrap();
    assert!(upload(store.plan_sticker_upload(&image, 101).unwrap()) == image_pending);
    assert!(store
        .finish_sticker_pack_upload(&image_pending, 101)
        .is_err());
    assert!(store.finish_sticker_upload(&pack_pending, 101).is_err());
    store.finish_sticker_upload(&image_pending, 101).unwrap();
    assert_eq!(used(&store), 580);
    let rows: i64 = store
        .db
        .query_row("SELECT COUNT(*) FROM `sticker_uploads`", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(rows, 2);
}
