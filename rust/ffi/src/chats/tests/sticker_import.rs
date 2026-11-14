//! Picked archives use the same core validator/sealed collection as desktop imports.
use super::favorites::Fixture;
use super::saved_sticker::{pack, save};
use super::*;
use crate::sticker_import_byte_limit;
use prost::Message as _;
use std::path::Path;

#[derive(Clone, PartialEq, prost::Message)]
struct Record {
    #[prost(bytes = "vec", tag = "1")]
    manifest: Vec<u8>,
    #[prost(bytes = "vec", repeated, tag = "2")]
    files: Vec<Vec<u8>>,
}

fn tar_entry(archive: &mut Vec<u8>, name: &str, bytes: &[u8]) {
    let mut header = [0; 512];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[124..136].copy_from_slice(format!("{:011o}\0", bytes.len()).as_bytes());
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..262].copy_from_slice(b"ustar");
    let sum: usize = header.iter().map(|byte| usize::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    archive.extend(header);
    archive.extend(bytes);
    archive.resize(archive.len().next_multiple_of(512), 0);
}

fn archive(corrupt: bool) -> Vec<u8> {
    let record = Record::decode(pack().encode().as_slice()).unwrap();
    let mut image = record.files.into_iter().next().unwrap();
    if corrupt {
        image[0] ^= 1;
    }
    let mut archive = Vec::new();
    tar_entry(&mut archive, "manifest.json", &record.manifest);
    tar_entry(&mut archive, "cat.webp", &image);
    archive.resize(archive.len() + 1024, 0);
    archive
}

fn no_outbox(f: &Fixture) {
    assert!(f
        .chats
        .session
        .lock()
        .unwrap()
        .store()
        .due_outbound(i64::MAX)
        .unwrap()
        .is_empty());
}

#[test]
fn native_import_is_sealed_idempotent_and_readable_by_collection_and_frame_apis() {
    let f = Fixture::new();
    let input = archive(false);
    let imported = f.chats.import_sticker_pack(input.clone()).unwrap();
    assert!(imported.added);
    assert_eq!(imported.pack_id, pack().id());
    let file = Path::new(&f.dir)
        .join("sticker-packs")
        .join(dyapp_identity::hex(&imported.pack_id));
    let sealed = std::fs::read(&file).unwrap();
    assert_ne!(sealed, input);
    let repeated = f.chats.import_sticker_pack(input).unwrap();
    assert!(!repeated.added);
    assert_eq!(repeated.pack_id, imported.pack_id);
    assert_eq!(std::fs::read(file).unwrap(), sealed);
    let page = f.chats.saved_sticker_packs(None).unwrap();
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].pack_id, imported.pack_id);
    assert_eq!(
        f.chats
            .saved_sticker_frame(imported.pack_id.clone(), 0)
            .unwrap()
            .emoji,
        "😺"
    );
    no_outbox(&f);
    drop(f.chats);
    let reopened =
        ChatSession::open(f.dir, f.key, f.device, Arc::new(TestStore), f.wrapped).unwrap();
    assert_eq!(
        reopened.saved_sticker_packs(None).unwrap().entries[0].pack_id,
        imported.pack_id
    );
}

#[test]
fn malformed_oversized_and_hash_mismatched_imports_create_no_saved_files_or_mail() {
    let f = Fixture::new();
    assert_eq!(
        sticker_import_byte_limit(),
        dyapp_client::sticker_tar::MAX_ARCHIVE as u64
    );
    for bytes in [
        Vec::new(),
        vec![0; usize::try_from(sticker_import_byte_limit()).unwrap() + 1],
        archive(true),
    ] {
        assert!(f.chats.import_sticker_pack(bytes).is_err());
    }
    assert!(!Path::new(&f.dir).join("sticker-packs").exists());
    no_outbox(&f);
}

#[test]
fn import_cannot_overwrite_a_foreign_saved_file_even_with_the_same_key() {
    let f = Fixture::new();
    let other = Fixture::new();
    let pack = pack();
    save(&other, &pack, &f.chats.seal_key);
    let name = dyapp_identity::hex(pack.id());
    let source = Path::new(&other.dir).join("sticker-packs").join(&name);
    let dir = Path::new(&f.dir).join("sticker-packs");
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join(name);
    std::fs::copy(source, &target).unwrap();
    let before = std::fs::read(&target).unwrap();
    assert!(f.chats.import_sticker_pack(archive(false)).is_err());
    assert_eq!(std::fs::read(target).unwrap(), before);
    no_outbox(&f);
}
