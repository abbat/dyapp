use super::*;

fn header(name: &str, kind: u8, size: usize) -> Vec<u8> {
    let mut block = vec![0u8; BLOCK];
    block[..name.len()].copy_from_slice(name.as_bytes());
    block[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
    block[148..156].fill(b' ');
    block[156] = kind;
    block[257..263].copy_from_slice(b"ustar\0");
    block[263..265].copy_from_slice(b"00");
    checksum(&mut block);
    block
}

fn checksum(block: &mut [u8]) {
    block[148..156].fill(b' ');
    let sum: usize = block.iter().map(|b| usize::from(*b)).sum();
    block[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
}

fn archive(name: &str, kind: u8, data: &[u8]) -> Vec<u8> {
    let mut bytes = header(name, kind, data.len());
    bytes.extend_from_slice(data);
    bytes.resize(BLOCK + data.len().div_ceil(BLOCK) * BLOCK + 2 * BLOCK, 0);
    bytes
}

#[test]
fn rejection_errors_escape_hostile_names_without_banning_unicode_files() {
    let error = read(&archive("cat\u{202e}.webp", b'5', b"")).unwrap_err();
    assert!(!error.contains('\u{202e}'), "{error:?}");
    assert!(error.contains("\\u{202e}"));
    let files = archive("кот.webp", b'0', b"image");
    assert_eq!(read(&files).unwrap()["кот.webp"], b"image");
}

#[test]
fn real_gnu_tar_keeps_exact_bytes() {
    let files = read(include_bytes!("fixtures/gnu.tar")).unwrap();
    assert_eq!(files["manifest.json"], b"{}");
    assert_eq!(files["cat.webp"], b"test");
}

#[test]
fn root_file_bytes_are_borrowed_and_pax_is_skipped() {
    let file = archive("cat.webp", b'0', b"test");
    let parsed = read(&file).unwrap();
    assert_eq!(parsed["cat.webp"], b"test");
    assert_eq!(parsed["cat.webp"].as_ptr(), file[BLOCK..].as_ptr());
    let mut pax = archive("PaxHeaders/cat.webp", b'x', b"ignored");
    pax.truncate(pax.len() - 2 * BLOCK);
    pax.extend_from_slice(&file);
    assert_eq!(read(&pax).unwrap()["cat.webp"], b"test");
}

#[test]
fn paths_sidecars_links_directories_and_duplicates_are_refused() {
    for name in [
        "",
        ".",
        "..",
        "../cat.webp",
        "/cat.webp",
        "dir/cat.webp",
        "._cat.webp",
        "a\\b",
    ] {
        assert!(read(&archive(name, b'0', b"x")).is_err(), "{name}");
    }
    for &kind in b"125LS" {
        assert!(read(&archive("cat.webp", kind, b"x")).is_err());
    }
    let mut duplicate = archive("cat.webp", b'0', b"x");
    duplicate.truncate(duplicate.len() - 2 * BLOCK);
    duplicate.extend_from_slice(&archive("cat.webp", b'0', b"y"));
    assert!(read(&duplicate).unwrap_err().contains("cat.webp"));
}

#[test]
fn numeric_and_prefix_guards_reject_independent_malformed_fields() {
    assert_eq!(octal(b" \0 ").unwrap_err(), "invalid TAR octal number");
    assert_eq!(octal(b"8").unwrap_err(), "invalid TAR octal number");
    assert_eq!(octal(b" 17\0 ").unwrap(), 15);
    let mut bytes = archive("cat.webp", b'0', b"x");
    bytes[345] = b'p'; // Nonempty USTAR prefix, with an otherwise valid root filename.
    checksum(&mut bytes[..BLOCK]);
    assert!(read(&bytes)
        .unwrap_err()
        .contains("only root regular files"));
}

#[test]
fn archive_and_file_limits_are_protocol_bytes() {
    for (name, limit) in [("manifest.json", 65_536), ("cat.webp", 262_144)] {
        let data = vec![0; limit];
        let bytes = archive(name, b'0', &data);
        assert_eq!(read(&bytes).unwrap()[name].len(), limit);
        let bytes = archive(name, b'0', &vec![0; limit + 1]);
        assert!(read(&bytes).unwrap_err().contains("import limit"));
    }
    // Zero end blocks plus padding are valid through exactly 8 MiB.
    assert!(read(&vec![0; 8_388_608]).is_ok());
    assert!(read(&vec![0; 8_388_608 + 512])
        .unwrap_err()
        .contains("too large"));
}

#[test]
fn checksum_truncation_sizes_and_end_blocks_are_checked() {
    let valid = archive("cat.webp", b'0', b"x");
    let mut corrupt = valid.clone();
    corrupt[0] ^= 1;
    assert!(read(&corrupt).is_err());
    for end in [0, 1, BLOCK, valid.len() - 1, valid.len() - BLOCK] {
        assert!(read(&valid[..end]).is_err(), "{end}");
    }
    assert!(read(&archive("cat.webp", b'0', &vec![0; MAX_FILE + 1])).is_err());
    let mut overflow = valid;
    overflow[124] = 0xff;
    checksum(&mut overflow[..BLOCK]);
    assert!(read(&overflow).is_err());
}
