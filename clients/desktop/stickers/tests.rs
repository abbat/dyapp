use super::*;

fn archive() -> Vec<u8> {
    archive_count(1)
}
pub(super) fn archive_count(count: usize) -> Vec<u8> {
    archive_labels(&vec!["😺"; count])
}
pub(super) fn archive_labels(labels: &[&str]) -> Vec<u8> {
    let image = include_bytes!("../../../tests/fuzz/corpus/sticker_webp/lossy.webp");
    let hash = crate::hex::encode(&dyapp_identity::sha256(image));
    let names: Vec<_> = (0..labels.len())
        .map(|index| format!("cat{index}.webp"))
        .collect();
    let stickers: Vec<_> = names
        .iter()
        .zip(labels)
        .map(|(name, emoji)| {
            serde_json::json!({
                "file":name,"emoji":emoji,"sha256":hash,
            })
        })
        .collect();
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format":1,"name":"Animals","stickers":stickers
    }))
    .unwrap();
    let mut archive = Vec::new();
    let entries = std::iter::once(("manifest.json", manifest.as_slice()))
        .chain(names.iter().map(|name| (name.as_str(), image.as_slice())));
    for (name, bytes) in entries {
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
    archive.resize(archive.len() + 1024, 0);
    archive
}
#[test]
fn imported_collection_reopens_with_the_slot_secret() {
    let root = std::env::current_dir()
        .unwrap()
        .join("build")
        .join(format!("sticker-native-{}", Identity::generate().peer_id()));
    std::fs::create_dir_all(&root).unwrap();
    let (slot, identity) = crate::slots::create(&root, "file", &crate::save, &crate::load).unwrap();
    let sessions = chats::Sessions::default();
    let packs = slot.dir.join("sticker-packs");
    std::fs::create_dir(&packs).unwrap();
    std::fs::write(packs.join("09".repeat(32)), b"corrupt pack").unwrap();
    assert!(!slot.seal_path().exists());
    assert!(remove(&slot.dir, &identity, &[9; 32]).unwrap());
    assert!(!slot.seal_path().exists());
    let archive = archive();
    let id = import(&root, &slot.dir, &identity, &sessions, &archive).unwrap();
    assert_eq!(
        import(&root, &slot.dir, &identity, &sessions, &archive).unwrap(),
        id
    );
    let (store, key) = local_store(&root, &slot.dir, &identity, &sessions).unwrap();
    let view = PageView::from(store.saved_sticker_packs(&key, None).unwrap());
    assert_eq!(view.entries.len(), 1);
    assert_eq!(view.entries[0].id, id);
    assert_eq!(view.entries[0].name.as_deref(), Some("Animals"));
    assert!(view.entries[0].error.is_none());
    drop(store);
    drop(key);
    let (store, key) = local_store(&root, &slot.dir, &identity, &sessions).unwrap();
    assert!(store.saved_sticker_pack(&key, &id).unwrap().is_some());
    drop(store);
    assert!(remove(&slot.dir, &identity, &id).unwrap());
    assert!(!remove(&slot.dir, &identity, &id).unwrap());
    let (store, key) = local_store(&root, &slot.dir, &identity, &sessions).unwrap();
    assert!(store.saved_sticker_pack(&key, &id).unwrap().is_none());
}

/// The system `tar` of macOS and Windows is libarchive bsdtar, so a pack made there must import.
#[cfg(any(target_os = "macos", windows))]
#[test]
fn a_pack_made_with_the_system_bsdtar_imports() {
    let tar = if cfg!(windows) {
        r"C:\Windows\System32\tar.exe"
    } else {
        "/usr/bin/tar"
    };
    let dir =
        std::env::temp_dir().join(format!("sticker-bsdtar-{}", Identity::generate().peer_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let reference = archive();
    let files = dyapp_client::sticker_tar::read(&reference).unwrap();
    for (name, bytes) in &files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    let run = |args: &[&str]| {
        let out = std::process::Command::new(tar)
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    assert!(run(&["--version"]).contains("bsdtar"));
    let mut create = vec!["-cf", "pack.tar"];
    create.extend(files.keys().copied());
    run(&create);
    let made = std::fs::read(dir.join("pack.tar")).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let pack = sticker_pack::import_tar(&made).unwrap();
    assert_eq!(
        pack.id(),
        sticker_pack::import_tar(&reference).unwrap().id()
    );
}
