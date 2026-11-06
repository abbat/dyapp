use super::*;
use image_webp::{ColorType, WebPEncoder};

fn image(pixel: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    WebPEncoder::new(&mut out)
        .encode(&pixel, 1, 1, ColorType::Rgba8)
        .unwrap();
    out
}

fn manifest(images: &[(&str, &[u8])]) -> Vec<u8> {
    let stickers: Vec<_> = images.iter().map(|(name, bytes)| serde_json::json!({
        "file": name, "emoji": "😺",
        "sha256": sha256(bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
    })).collect();
    serde_json::to_vec(&serde_json::json!({"format":1,"name":"Cats","stickers":stickers})).unwrap()
}

fn tar(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, bytes) in files {
        let mut header = [0; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[124..136].copy_from_slice(format!("{:011o}\0", bytes.len()).as_bytes());
        header[148..156].fill(b' ');
        header[156] = b'0';
        header[257..262].copy_from_slice(b"ustar");
        let checksum: usize = header.iter().map(|byte| usize::from(*byte)).sum();
        header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
        out.extend(header);
        out.extend_from_slice(bytes);
        out.resize(out.len().next_multiple_of(512), 0);
    }
    out.resize(out.len() + 1024, 0);
    out
}

#[test]
fn tar_and_peer_record_have_identical_identity_and_manifest_order() {
    let red = image([255, 0, 0, 255]);
    let blue = image([0, 0, 255, 255]);
    let manifest = manifest(&[("z.webp", &red), ("a.webp", &blue)]);
    let archive = tar(&[
        ("a.webp", &blue),
        ("manifest.json", &manifest),
        ("z.webp", &red),
    ]);
    let pack = import_tar(&archive).unwrap();
    assert_eq!(pack.id(), &sha256(&manifest));
    assert_eq!(pack.name(), "Cats");
    assert_eq!(pack.stickers()[0].file, "z.webp");
    assert_eq!(pack.sticker_bytes(0), Some(red.as_slice()));
    assert!(pack.sticker_bytes(2).is_none());
    let encoded = pack.encode();
    let peer = from_peer(&encoded, pack.id()).unwrap();
    assert_eq!(peer.encode(), encoded);
    assert_eq!(peer.id(), pack.id());
    assert!(from_peer(&encoded, &[0; 32]).is_err());
    let mut extension = encoded.clone();
    extension.extend([24, 1]); // Unknown field3 is forward-compatible.
    assert_eq!(from_peer(&extension, pack.id()).unwrap().encode(), encoded);
}

#[test]
fn common_validator_refuses_missing_extra_corrupt_and_transparent_files() {
    let image = image([0; 4]);
    let manifest = manifest(&[("cat.webp", &image)]);
    let record = Record {
        manifest: manifest.clone(),
        files: vec![image.clone()],
    };
    assert!(validate(record, None).err().unwrap().contains("cat.webp"));
    let archive = tar(&[("manifest.json", &manifest), ("cat.webp", &image)]);
    assert!(import_tar(&archive).err().unwrap().contains("cat.webp"));
    let record = Record {
        manifest: manifest.clone(),
        files: vec![],
    };
    assert!(from_peer(&record.encode_to_vec(), &sha256(&manifest))
        .err()
        .unwrap()
        .contains("cat.webp"));
    let record = Record {
        manifest: manifest.clone(),
        files: vec![b"corrupt".to_vec()],
    };
    assert!(validate(record, None).err().unwrap().contains("cat.webp"));
    let archive = tar(&[
        ("manifest.json", &manifest),
        ("cat.webp", &image),
        ("extra.webp", &image),
    ]);
    assert!(import_tar(&archive).err().unwrap().contains("extra.webp"));
}

#[test]
fn protobuf_preflight_rejects_repetition_lengths_and_duplicate_manifest() {
    let mut bytes = Record {
        manifest: b"x".to_vec(),
        files: vec![vec![1]],
    }
    .encode_to_vec();
    bytes.extend([10, 1, b'x']);
    assert!(preflight(&bytes).err().unwrap().contains("duplicate"));
    let mut bytes = vec![10, 1, b'x'];
    for _ in 0..MAX_STICKERS + 1 {
        bytes.extend([18, 1, 0]);
    }
    assert!(preflight(&bytes).err().unwrap().contains("count"));
    for bad in [vec![10, 127, 0], vec![8, 1], vec![18, 255], vec![0]] {
        assert!(preflight(&bad).is_err());
    }
}

#[test]
fn sealed_size_includes_every_chunk_and_protobuf_field() {
    let overhead = MAX_SEALED_BYTES.div_ceil(crate::attachment::CHUNK_BYTES) * 16;
    let max_plaintext = MAX_SEALED_BYTES - overhead;
    assert_eq!(sealed_size(max_plaintext), Some(MAX_SEALED_BYTES));
    check_size(max_plaintext).unwrap();
    assert!(check_size(max_plaintext + 1).is_err());
    assert!(check_size(usize::MAX).is_err());
    let record = Record {
        manifest: b"x".to_vec(),
        files: vec![vec![0; MAX_STICKER_BYTES]; 24],
    };
    assert!(validate(record, None)
        .err()
        .unwrap()
        .contains("sealed sticker pack"));
}
