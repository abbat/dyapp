//! Bounded, in-memory sticker TAR import. No archive path is ever opened on disk.

use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

const BLOCK: usize = 512;
/// Maximum picked TAR bytes, checked before archive parsing.
pub const MAX_ARCHIVE: usize = 8 * 1024 * 1024;
const MAX_FILES: usize = 121;
const MAX_FILE: usize = 256 * 1024;
const MAX_MANIFEST: usize = 64 * 1024;

fn octal(field: &[u8]) -> Result<usize, String> {
    let text = std::str::from_utf8(field).map_err(|_| "non-ASCII TAR number")?;
    let text = text.trim_matches([' ', '\0']);
    if text.is_empty() || !text.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return Err("invalid TAR octal number".into());
    }
    usize::from_str_radix(text, 8).map_err(|_| "TAR number overflow".into())
}

fn name(header: &[u8]) -> Result<&str, String> {
    let end = header[..100].iter().position(|b| *b == 0).unwrap_or(100);
    let name = std::str::from_utf8(&header[..end]).map_err(|_| "non-UTF8 TAR filename")?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.starts_with("._")
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
        || header[345..500].iter().any(|b| *b != 0)
    {
        return Err(format!("{name:?}: only root regular files are allowed"));
    }
    Ok(name)
}

fn checked_header(header: &[u8]) -> Result<(), String> {
    let expected = octal(&header[148..156])?;
    let actual: usize = header
        .iter()
        .enumerate()
        .map(|(at, byte)| {
            if (148..156).contains(&at) {
                32
            } else {
                usize::from(*byte)
            }
        })
        .sum();
    if expected != actual {
        return Err("TAR checksum mismatch".into());
    }
    if &header[257..262] != b"ustar" {
        return Err("unsupported TAR header".into());
    }
    Ok(())
}

fn entry_data(bytes: &[u8], offset: usize, size: usize) -> Result<(&[u8], usize), String> {
    let start = offset + BLOCK;
    let end = start.checked_add(size).ok_or("TAR size overflow")?;
    let next = start
        .checked_add(
            size.div_ceil(BLOCK)
                .checked_mul(BLOCK)
                .ok_or("TAR padding overflow")?,
        )
        .ok_or("TAR offset overflow")?;
    let data = bytes.get(start..end).ok_or("truncated TAR file")?;
    if next > bytes.len() {
        return Err("truncated TAR padding".into());
    }
    Ok((data, next))
}

/// Returns borrowed root-file bytes after bounded header, checksum and path validation.
/// PAX metadata is skipped, never interpreted as a path override. Manifest membership and
/// content hashes are validated by the caller before storing or decoding any returned file.
///
/// # Errors
///
/// Returns an error if archive size or alignment, headers, checksums, filenames,
/// entry types, file/count limits, offsets, padding, or end blocks are invalid.
pub fn read(bytes: &[u8]) -> Result<BTreeMap<&str, &[u8]>, String> {
    if bytes.len() > MAX_ARCHIVE || !bytes.len().is_multiple_of(BLOCK) {
        return Err("TAR archive too large or not block aligned".into());
    }
    let mut files = BTreeMap::new();
    let mut offset = 0;
    while let Some(header) = bytes.get(offset..offset + BLOCK) {
        if header.iter().all(|b| *b == 0) {
            let tail = bytes.get(offset + BLOCK..).ok_or("truncated TAR end")?;
            if tail.len() < BLOCK || tail.iter().any(|b| *b != 0) {
                return Err("invalid TAR end blocks".into());
            }
            return Ok(files);
        }
        checked_header(header)?;
        let size = octal(&header[124..136])?;
        let (data, next) = entry_data(bytes, offset, size)?;
        let kind = header[156];
        if kind == b'x' || kind == b'g' {
            if size > MAX_MANIFEST {
                return Err("PAX metadata too large".into());
            }
        } else {
            let name = name(header)?;
            if kind != 0 && kind != b'0' {
                return Err(format!("{name:?}: unsupported TAR entry type"));
            }
            let limit = if name == "manifest.json" {
                MAX_MANIFEST
            } else {
                MAX_FILE
            };
            if size > limit || files.len() >= MAX_FILES {
                return Err(format!("{name:?}: sticker import limit exceeded"));
            }
            if files.insert(name, data).is_some() {
                return Err(format!("{name:?}: duplicate TAR filename"));
            }
        }
        offset = next;
    }
    Err("missing TAR end blocks".into())
}
