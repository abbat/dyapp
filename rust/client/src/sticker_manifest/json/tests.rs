use super::*;
use dyapp_identity::sha256;

fn source(name: &str, emoji: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "format": 1, "name": name,
        "stickers": [{"file": "cat.webp", "sha256": dyapp_identity::hex(&sha256(b"image")), "emoji": emoji}]
    }))
    .unwrap()
}

#[test]
fn parsed_entries_bind_files_to_original_bytes_and_order() {
    let bytes = source("Cats", "😺");
    let parsed = parse(&bytes).unwrap();
    assert_eq!(parsed.name(), "Cats");
    assert_eq!(parsed.stickers()[0].emoji, "😺");
    assert_eq!(parsed.stickers()[0].sha256, sha256(b"image"));
    let mut files = BTreeMap::from([
        ("manifest.json", bytes.as_slice()),
        ("cat.webp", b"image".as_slice()),
    ]);
    let checked = parsed.check_files(&files).unwrap();
    assert_eq!(checked.pack_id, sha256(&bytes));
    assert_eq!(checked.stickers, [b"image"]);
    files.insert("cat.webp", b"wrong");
    assert!(parsed
        .check_files(&files)
        .err()
        .unwrap()
        .contains("cat.webp"));
    files.insert("cat.webp", b"image");
    let mut changed = bytes.clone();
    changed.push(b' ');
    files.insert("manifest.json", &changed);
    assert!(parsed.check_files(&files).is_err());
    assert_ne!(
        parse(&changed)
            .unwrap()
            .check_files(&files)
            .unwrap()
            .pack_id,
        checked.pack_id
    );
}

#[test]
fn labels_use_clean_nfc_and_extended_graphemes() {
    for emoji in ["😺", "🇩🇪", "👩\u{200d}💻", "👍🏽"] {
        assert!(parse(&source("Cats", emoji)).is_ok(), "{emoji}");
    }
    for emoji in ["", " ", "😺😺", "a\n", "\u{202e}", "e\u{301}"] {
        assert!(parse(&source("Cats", emoji)).is_err(), "{emoji:?}");
    }
    assert!(parse(&source(&"a".repeat(64), "😺")).is_ok());
    for name in [
        String::new(),
        " ".into(),
        "a\n".into(),
        "a\u{202e}".into(),
        "e\u{301}".into(),
        "a".repeat(65),
    ] {
        assert!(parse(&source(&name, "😺")).is_err(), "{name:?}");
    }
}

#[test]
fn schema_duplicate_fields_and_unknown_format_are_distinct() {
    assert!(parse(br#"{"format":2}"#)
        .err()
        .unwrap()
        .contains("update the app"));
    for bad in [
        b"".as_slice(),
        b"null",
        b"[]",
        b"{}",
        b"{",
        b"\xff",
        br#"{"format":1,"format":1}"#,
        br#"{"format":-1}"#,
        br#"{"format":1.5}"#,
    ] {
        assert!(parse(bad).is_err());
    }
    let bytes = source("Cats", "😺");
    let original = std::str::from_utf8(&bytes).unwrap();
    for duplicate in [
        original.replacen("\"name\":", "\"name\":\"Other\",\"name\":", 1),
        original.replacen("\"file\":", "\"file\":\"other.webp\",\"file\":", 1),
    ] {
        assert!(parse(duplicate.as_bytes()).is_err());
    }
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["extension"] = serde_json::json!({"future": true});
    assert!(parse(&serde_json::to_vec(&value).unwrap()).is_ok());
    value["stickers"] = serde_json::json!([]);
    assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut oversized = vec![b' '; MAX_MANIFEST_BYTES + 1];
    oversized[..bytes.len()].copy_from_slice(&bytes);
    assert!(parse(&oversized).is_err());
}

#[test]
fn hashes_paths_and_entry_count_are_bounded() {
    let mut value: serde_json::Value = serde_json::from_slice(&source("Cats", "😺")).unwrap();
    for invalid in [
        "0".repeat(63),
        "0".repeat(65),
        "g".repeat(64),
        "é".repeat(32),
    ] {
        value["stickers"][0]["sha256"] = invalid.into();
        assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    value["stickers"][0]["sha256"] = "AB".repeat(32).into();
    assert_eq!(
        parse(&serde_json::to_vec(&value).unwrap())
            .unwrap()
            .stickers()[0]
            .sha256,
        [0xab; 32]
    );
    value["stickers"][0]["file"] = "../cat.webp".into();
    assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
    let entry = serde_json::json!({"file":"cat.webp","sha256":"00".repeat(32),"emoji":"😺"});
    value["stickers"] = serde_json::Value::Array(vec![entry; MAX_STICKERS + 1]);
    assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
}
