use super::*;
use image_webp::{ColorType, WebPEncoder};
use prost::Message;

fn store(owner: &str, path: Option<&Path>) -> Store {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build")
        .join(format!(
            "saved-pack-test-{}",
            dyapp_identity::Identity::generate().peer_id()
        ));
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => {
            fs::create_dir_all(&dir).unwrap();
            dir.join("client.db")
        }
    };
    Store {
        db: rusqlite::Connection::open(path).unwrap(),
        owner_peer_id: owner.into(),
    }
}
pub(crate) fn pack(name: &str) -> Pack {
    let mut image = Vec::new();
    WebPEncoder::new(&mut image)
        .encode(&[1, 2, 3, 255], 1, 1, ColorType::Rgba8)
        .unwrap();
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format":1,"name":name,"stickers":[{
            "file":"cat.webp","emoji":"😺","sha256":hex(&sha256(&image))
        }]
    }))
    .unwrap();
    #[derive(Clone, PartialEq, Message)]
    struct Record {
        #[prost(bytes, tag = "1")]
        manifest: Vec<u8>,
        #[prost(bytes, repeated, tag = "2")]
        files: Vec<Vec<u8>>,
    }
    let id = sha256(&manifest);
    sticker_pack::from_peer(
        &Record {
            manifest,
            files: vec![image],
        }
        .encode_to_vec(),
        &id,
    )
    .unwrap()
}

#[test]
fn encrypted_roundtrip_restart_and_idempotence() {
    let mut first = store("alice", None);
    let pack = pack("Private collection name");
    assert!(!first.has_sticker_packs().unwrap());
    assert!(first.save_sticker_pack(&[7; 32], &pack).unwrap());
    let path = directory(&first).unwrap().join(hex(pack.id()));
    let bytes = fs::read(&path).unwrap();
    assert!(!bytes
        .windows(pack.name().len())
        .any(|part| part == pack.name().as_bytes()));
    assert!(!first.save_sticker_pack(&[7; 32], &pack).unwrap());
    let db = PathBuf::from(first.db.path().unwrap());
    drop(first);
    let reopened = store("alice", Some(&db));
    assert!(reopened.has_sticker_packs().unwrap());
    assert_eq!(
        reopened
            .saved_sticker_pack(&[7; 32], pack.id())
            .unwrap()
            .unwrap()
            .encode(),
        pack.encode()
    );
    assert!(reopened.saved_sticker_pack(&[8; 32], pack.id()).is_err());
    let foreign = store("bob", Some(&db));
    assert!(foreign.saved_sticker_pack(&[7; 32], pack.id()).is_err());
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn quota_and_pending_files_never_evict_saved_content() {
    let mut store = store("alice", None);
    let first = pack("First");
    let second = pack("Second");
    store.save_sticker_pack(&[7; 32], &first).unwrap();
    let dir = directory(&store).unwrap();
    let path = dir.join(hex(first.id()));
    let before = fs::read(&path).unwrap();
    let second_size = HEADER + attachment::sealed_size(second.encode().len()).unwrap();
    let limit = before.len() as u64 + second_size as u64;
    fs::write(dir.join(".interrupted.pending"), [0]).unwrap();
    assert!(matches!(
        store.save_pack_with_limit(&[7; 32], &second, limit),
        Err(Error::Quota)
    ));
    assert_eq!(fs::read(path).unwrap(), before);
    assert!(!dir.join(hex(second.id())).exists());
    assert!(store
        .save_pack_with_limit(&[7; 32], &second, limit + 1)
        .unwrap());
}

#[test]
fn explicit_removal_recovers_quota_and_is_identity_local() {
    let mut owner = store("alice", None);
    let mut foreign = store("bob", None);
    let first = pack("First");
    let other = pack("Other");
    assert!(!owner.remove_saved_sticker_pack(first.id()).unwrap());
    owner.save_sticker_pack(&[7; 32], &first).unwrap();
    foreign.save_sticker_pack(&[8; 32], &first).unwrap();
    let dir = directory(&owner).unwrap();
    fs::write(dir.join(".interrupted.pending"), [0]).unwrap();
    let limit = used(&dir).unwrap();
    assert!(matches!(
        owner.save_pack_with_limit(&[7; 32], &other, limit),
        Err(Error::Quota)
    ));
    assert!(owner.remove_saved_sticker_pack(first.id()).unwrap());
    assert!(!owner.remove_saved_sticker_pack(first.id()).unwrap());
    assert!(owner
        .saved_sticker_pack(&[7; 32], first.id())
        .unwrap()
        .is_none());
    assert!(owner.save_pack_with_limit(&[7; 32], &other, limit).unwrap());
    assert!(foreign
        .saved_sticker_pack(&[8; 32], first.id())
        .unwrap()
        .is_some());
    assert_eq!(fs::read(dir.join(".interrupted.pending")).unwrap(), [0]);
    let path = PathBuf::from(owner.db.path().unwrap());
    drop(owner);
    let reopened = store("alice", Some(&path));
    assert!(reopened
        .saved_sticker_pack(&[7; 32], first.id())
        .unwrap()
        .is_none());
    assert!(reopened
        .saved_sticker_pack(&[7; 32], other.id())
        .unwrap()
        .is_some());
}

#[test]
fn explicit_removal_can_recover_a_corrupt_pack_without_its_key() {
    let mut store = store("alice", None);
    let first = pack("First");
    let other = pack("Other");
    store.save_sticker_pack(&[7; 32], &first).unwrap();
    store.save_sticker_pack(&[7; 32], &other).unwrap();
    let dir = directory(&store).unwrap();
    let kept = fs::read(dir.join(hex(other.id()))).unwrap();
    fs::write(dir.join(hex(first.id())), b"broken").unwrap();
    assert!(store.remove_saved_sticker_pack(first.id()).unwrap());
    assert_eq!(fs::read(dir.join(hex(other.id()))).unwrap(), kept);
}

#[test]
fn corrupt_content_is_preserved_and_cannot_be_overwritten() {
    let mut store = store("alice", None);
    let pack = pack("Cats");
    store.save_sticker_pack(&[7; 32], &pack).unwrap();
    let path = directory(&store).unwrap().join(hex(pack.id()));
    fs::write(&path, b"broken").unwrap();
    assert!(store.saved_sticker_pack(&[7; 32], pack.id()).is_err());
    assert!(store.save_sticker_pack(&[7; 32], &pack).is_err());
    assert_eq!(fs::read(path).unwrap(), b"broken");
}

#[test]
fn sqlite_writer_lock_prevents_overlapping_publications() {
    let mut first = store("alice", None);
    let db = PathBuf::from(first.db.path().unwrap());
    let mut second = store("alice", Some(&db));
    second.db.busy_timeout(std::time::Duration::ZERO).unwrap();
    let lock = first
        .db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(matches!(
        second.save_sticker_pack(&[7; 32], &pack("Cats")),
        Err(Error::Sql(_))
    ));
    assert!(matches!(
        second.remove_saved_sticker_pack(pack("Cats").id()),
        Err(Error::Sql(_))
    ));
    drop(lock);
    assert!(second.save_sticker_pack(&[7; 32], &pack("Cats")).unwrap());
}

#[test]
fn collection_pages_are_ordered_bounded_and_keep_unreadable_entries() {
    let mut store = store("alice", None);
    assert!(store
        .saved_sticker_packs(&[7; 32], None)
        .unwrap()
        .entries
        .is_empty());
    let mut ids = Vec::new();
    for index in 0..34 {
        let pack = pack(&format!("Cats {index}"));
        store.save_sticker_pack(&[7; 32], &pack).unwrap();
        ids.push(*pack.id());
    }
    ids.sort();
    let dir = directory(&store).unwrap();
    fs::write(dir.join(hex(&ids[1])), b"broken").unwrap();
    fs::write(dir.join(".interrupted.pending"), b"pending").unwrap();
    let first = store.saved_sticker_packs(&[7; 32], None).unwrap();
    assert_eq!(
        first
            .entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        ids[..32]
    );
    assert_eq!(first.next, Some(ids[31]));
    assert_eq!(first.unrecognized_files, 1);
    assert_eq!(first.used_bytes, used(&dir).unwrap());
    assert!(first.entries[1].details.is_err());
    assert_eq!(first.entries[1].bytes, 6);
    let details = first.entries[0].details.as_ref().unwrap();
    assert!(details.name.starts_with("Cats "));
    assert_eq!(details.stickers, 1);
    let next = store
        .saved_sticker_packs(&[7; 32], first.next.as_ref())
        .unwrap();
    assert_eq!(
        next.entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        ids[32..]
    );
    assert!(next.next.is_none());
    let wrong_key = store.saved_sticker_packs(&[8; 32], None).unwrap();
    assert!(wrong_key.entries.iter().all(|entry| entry.details.is_err()));
    assert_eq!(fs::read(dir.join(hex(&ids[1]))).unwrap(), b"broken");
    let db = PathBuf::from(store.db.path().unwrap());
    drop(store);
    let reopened = self::store("alice", Some(&db));
    assert_eq!(
        reopened.saved_sticker_packs(&[7; 32], None).unwrap().next,
        first.next
    );
}

#[cfg(unix)]
#[test]
fn symlinks_are_refused_without_touching_the_target() {
    use std::os::unix::fs::symlink;
    let mut store = store("alice", None);
    let dir = directory(&store).unwrap();
    fs::create_dir(&dir).unwrap();
    let pack = pack("Cats");
    let target = dir.parent().unwrap().join("unrelated");
    fs::write(&target, b"keep").unwrap();
    symlink(&target, dir.join(hex(pack.id()))).unwrap();
    assert!(store.save_sticker_pack(&[7; 32], &pack).is_err());
    assert!(store.saved_sticker_pack(&[7; 32], pack.id()).is_err());
    assert!(store.remove_saved_sticker_pack(pack.id()).is_err());
    assert_eq!(fs::read(target).unwrap(), b"keep");
    let mut other = self::store("bob", None);
    let foreign_dir = directory(&other)
        .unwrap()
        .parent()
        .unwrap()
        .join("unrelated-packs");
    fs::create_dir(&foreign_dir).unwrap();
    let foreign_file = foreign_dir.join(hex(pack.id()));
    fs::write(&foreign_file, b"keep directory target").unwrap();
    symlink(&foreign_dir, directory(&other).unwrap()).unwrap();
    assert!(other.remove_saved_sticker_pack(pack.id()).is_err());
    assert_eq!(fs::read(foreign_file).unwrap(), b"keep directory target");
}
