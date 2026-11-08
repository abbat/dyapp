use super::*;

fn entry<'a>(name: &'a str, bytes: &[u8]) -> File<'a> {
    File {
        name,
        sha256: sha256(bytes),
    }
}

fn rejected(manifest: &[u8], entries: &[File<'_>], files: &BTreeMap<&str, &[u8]>, reason: &str) {
    let error = check_files(manifest, entries, files)
        .err()
        .expect("must reject");
    assert!(error.contains(reason), "{error}");
}

#[test]
fn exact_files_keep_manifest_order_and_original_identity() {
    let manifest = b"original JSON bytes".as_slice();
    let files = BTreeMap::from([
        ("manifest.json", manifest),
        ("a.webp", b"first".as_slice()),
        ("z.webp", b"last".as_slice()),
    ]);
    let entries = [entry("z.webp", b"last"), entry("a.webp", b"first")];
    let checked = check_files(manifest, &entries, &files).unwrap();
    assert_eq!(checked.stickers, [b"last".as_slice(), b"first"]);
    assert_eq!(checked.pack_id, sha256(manifest));
    let mut changed = files.clone();
    changed.insert("manifest.json", b"original JSON bytes ");
    assert_ne!(
        check_files(b"original JSON bytes ", &entries, &changed)
            .unwrap()
            .pack_id,
        checked.pack_id
    );
    rejected(manifest, &entries, &changed, "manifest.json");
}

#[test]
fn membership_digests_and_unsafe_names_fail_by_name() {
    let manifest = b"m".as_slice();
    let mut files = BTreeMap::from([
        ("manifest.json", manifest),
        ("cat.webp", b"image".as_slice()),
    ]);
    let entries = [entry("cat.webp", b"image")];
    rejected(manifest, &[entry("cat.webp", b"wrong")], &files, "cat.webp");
    rejected(
        manifest,
        &[entry("gone.webp", b"image")],
        &files,
        "gone.webp",
    );
    rejected(
        manifest,
        &[entry("cat.webp", b"image"), entry("cat.webp", b"image")],
        &files,
        "duplicate",
    );
    files.insert("extra.webp", b"other");
    rejected(manifest, &entries, &files, "extra.webp");
    for name in [
        "",
        ".",
        "..",
        "manifest.json",
        "../cat.webp",
        "dir/cat.webp",
        "dir\\cat.webp",
        "._cat.webp",
        "cat\0.webp",
        "cat\u{202e}.webp",
        "e\u{301}.webp",
    ] {
        rejected(
            manifest,
            &[entry(name, b"image")],
            &files,
            "unsafe root filename",
        );
    }
    let long = "x".repeat(101);
    rejected(
        manifest,
        &[entry(&long, b"image")],
        &files,
        "unsafe root filename",
    );
    // A format check, not a compressed image check: these bytes deliberately are not WebP.
    assert!(check_files(
        manifest,
        &entries,
        &BTreeMap::from([
            ("manifest.json", manifest),
            ("cat.webp", b"image".as_slice()),
        ])
    )
    .is_ok());
}

#[test]
fn manifest_count_and_sticker_byte_limits_are_inclusive() {
    // Literal protocol limits deliberately do not follow a mutated production constant.
    let max_manifest = vec![b'm'; 65_536];
    let max_sticker = vec![7; MAX_STICKER_BYTES];
    let entries = [entry("cat.webp", &max_sticker)];
    let files = BTreeMap::from([
        ("manifest.json", max_manifest.as_slice()),
        ("cat.webp", max_sticker.as_slice()),
    ]);
    assert!(check_files(&max_manifest, &entries, &files).is_ok());
    let too_big = vec![0; MAX_MANIFEST_BYTES + 1];
    rejected(&too_big, &entries, &files, "manifest size");
    rejected(b"", &entries, &files, "manifest size");
    rejected(&max_manifest, &[], &files, "sticker count");
    let big_sticker = vec![7; MAX_STICKER_BYTES + 1];
    let files = BTreeMap::from([
        ("manifest.json", b"m".as_slice()),
        ("cat.webp", big_sticker.as_slice()),
    ]);
    rejected(b"m", &entries, &files, "invalid sticker byte size");
    let files = BTreeMap::from([
        ("manifest.json", b"m".as_slice()),
        ("cat.webp", b"".as_slice()),
    ]);
    rejected(b"m", &entries, &files, "invalid sticker byte size");
    let names: Vec<_> = (0..=MAX_STICKERS).map(|i| format!("{i}.webp")).collect();
    let entries: Vec<_> = names.iter().map(|name| entry(name, b"x")).collect();
    let mut files: BTreeMap<_, _> = names
        .iter()
        .map(|name| (name.as_str(), b"x".as_slice()))
        .collect();
    files.insert("manifest.json", b"m");
    assert!(check_files(b"m", &entries[..MAX_STICKERS], &files).is_err()); // extra file
    let last = names.last().unwrap().as_str();
    files.remove(last);
    assert!(check_files(b"m", &entries[..MAX_STICKERS], &files).is_ok());
    rejected(b"m", &entries, &files, "sticker count");
}

#[test]
fn aggregate_bound_reports_overflow_before_sealing() {
    let names: Vec<_> = (0..24).map(|i| format!("{i}.webp")).collect();
    let bytes = vec![0; MAX_STICKER_BYTES];
    let entries: Vec<_> = names.iter().map(|name| entry(name, &bytes)).collect();
    let mut files: BTreeMap<_, _> = names
        .iter()
        .map(|name| (name.as_str(), bytes.as_slice()))
        .collect();
    files.insert("manifest.json", b"m");
    rejected(b"m", &entries, &files, "exceeds 6 MiB by 1 bytes");
    let short = &bytes[..bytes.len() - 1];
    let mut entries = entries;
    entries[0] = entry(&names[0], short);
    files.insert(names[0].as_str(), short);
    assert!(check_files(b"m", &entries, &files).is_ok());
}
