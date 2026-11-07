use super::*;
mod adversarial;
mod packs;
use dyapp_identity::Identity;

fn database() -> (Store, Identity, std::path::PathBuf) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/sticker-upload-tests");
    std::fs::create_dir_all(&root).unwrap();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = root.join(format!("{:032x}.db", u128::from_be_bytes(random)));
    let owner = Identity::generate();
    (Store::open(&path, &owner).unwrap(), owner, path)
}
fn candidate(byte: u8, size: usize) -> Attachment {
    let (mut attachment, _) = attachment::seal(&vec![byte; size]).unwrap();
    attachment.mime = "image/webp".into();
    attachment.width = 1;
    attachment.height = 1;
    attachment
}
fn upload(plan: UploadPlan) -> PendingUpload {
    match plan {
        UploadPlan::Upload(value) => value,
        _ => panic!("upload not admitted"),
    }
}
fn used(store: &Store) -> i64 {
    store
        .db
        .query_row(
            "SELECT COALESCE(SUM(`charge`), 0) FROM `sticker_uploads` WHERE `kind` = 1",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn charged_quota_includes_gcm_overhead_and_exact_integer_boundary() {
    assert_eq!(encode(&candidate(1, 1024), Kind::Sticker).unwrap().1, 5200);
    assert_eq!(
        encode(&candidate(1, 262144), Kind::Sticker).unwrap().1,
        1311040
    );
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
    assert_eq!(used(&store), 16777215);
    assert!(matches!(
        store.plan_sticker_upload(&candidate(21, 1), 100).unwrap(),
        UploadPlan::Fallback
    ));
    assert_eq!(used(&store), 16777215);
}

#[test]
fn restart_refresh_resume_expiry_and_stale_completion_keep_one_descriptor() {
    let (mut store, owner, path) = database();
    let first = candidate(1, 100);
    let newer = candidate(1, 100);
    let reserved = upload(store.plan_sticker_upload(&first, 100).unwrap());
    let resumed = upload(store.plan_sticker_upload(&newer, 101).unwrap());
    assert!(resumed == reserved && resumed.attachment.key != newer.key);
    store.finish_sticker_upload(&reserved, 102).unwrap();
    drop(store);
    let mut store = Store::open(&path, &owner).unwrap();
    assert!(
        matches!(store.plan_sticker_upload(&newer, 102 + 6 * 86400).unwrap(),
        UploadPlan::Reuse(ref value) if *value == reserved.attachment)
    );
    let refreshed = upload(store.plan_sticker_upload(&newer, 103 + 6 * 86400).unwrap());
    assert!(
        refreshed.attachment == reserved.attachment
            && refreshed.reservation != reserved.reservation
    );
    assert!(store
        .finish_sticker_upload(&reserved, 104 + 6 * 86400)
        .is_err());
    store
        .finish_sticker_upload(&refreshed, 104 + 6 * 86400)
        .unwrap();
    assert!(store.plan_sticker_upload(&candidate(2, 100), 100).is_err());
    let time = 105 + 14 * 86400;
    let replacement = upload(store.plan_sticker_upload(&newer, time).unwrap());
    assert!(replacement.attachment == newer);
    assert!(store.finish_sticker_upload(&reserved, time).is_err());
    let next = upload(store.plan_sticker_upload(&newer, time + 9 * 86400).unwrap());
    assert!(
        next.attachment == replacement.attachment && next.reservation != replacement.reservation
    );
    assert!(store
        .finish_sticker_upload(&replacement, time + 9 * 86400)
        .is_err());
    assert_eq!(used(&store), encode(&newer, Kind::Sticker).unwrap().1);
}
