//! Sticker manifest JSON decoding and structural file/digest preflight, without WebP validation.
//! Use [`json::parse`] to bind decoded entries to original manifest bytes before file checks.

use std::collections::{BTreeMap, BTreeSet};

use dyapp_identity::sha256;
use dyapp_profile::is_clean_text;

/// Typed JSON and Unicode validation before the common file checks.
pub mod json;

/// Maximum original JSON manifest size.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
/// Maximum number of listed stickers.
pub const MAX_STICKERS: usize = 120;
/// Maximum one sticker's compressed bytes.
pub const MAX_STICKER_BYTES: usize = 256 * 1024;
// Necessary plaintext bound only: protobuf and sealing overhead must also fit later.
const MAX_PACK_BYTES: usize = 6 * 1024 * 1024;

/// One file reference produced by the manifest parser.
pub struct File<'a> {
    /// Exact root filename; case is significant.
    pub name: &'a str,
    /// Digest decoded from the manifest's SHA-256 field.
    pub sha256: [u8; 32],
}

/// Files in manifest order after membership, byte-bound and digest checks.
/// Content and display text still need validation before storage or rendering.
pub struct Files<'a> {
    /// SHA-256 of the original manifest bytes, without reserialization.
    pub pack_id: [u8; 32],
    /// Borrowed sticker bytes, in the order the manifest declares.
    pub stickers: Vec<&'a [u8]>,
}

fn safe_file(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !matches!(name, "." | ".." | "manifest.json")
        && !name.starts_with("._")
        && !name.contains(['/', '\\'])
        && is_clean_text(name)
}

/// Checks a decoded manifest against root files, including `manifest.json`.
/// No path is opened. Both an importer and a peer-record decoder must call the
/// same checks, then validate manifest text, WebP pixels and the sealed record size.
///
/// # Errors
///
/// Returns an error if manifest bytes, counts, filenames, membership, sizes, or
/// hashes are invalid, or summing file sizes overflows.
pub fn check_files<'a>(
    manifest: &[u8],
    entries: &[File<'_>],
    files: &BTreeMap<&str, &'a [u8]>,
) -> Result<Files<'a>, String> {
    if manifest.is_empty() || manifest.len() > MAX_MANIFEST_BYTES {
        return Err("manifest.json: invalid manifest size".into());
    }
    if files.get("manifest.json").copied() != Some(manifest) {
        return Err("manifest.json: missing or mismatched original bytes".into());
    }
    if entries.is_empty() || entries.len() > MAX_STICKERS {
        return Err("manifest.json: sticker count must be 1..=120".into());
    }
    let mut names = BTreeSet::new();
    let mut stickers = Vec::with_capacity(entries.len());
    let mut total = manifest.len();
    for entry in entries {
        let name = entry.name;
        if !safe_file(name) {
            return Err(format!("{name:?}: unsafe root filename"));
        }
        if !names.insert(name) {
            return Err(format!("{name:?}: duplicate manifest filename"));
        }
        let bytes = files
            .get(name)
            .copied()
            .ok_or_else(|| format!("{name:?}: missing file"))?;
        if bytes.is_empty() || bytes.len() > MAX_STICKER_BYTES {
            return Err(format!("{name:?}: invalid sticker byte size"));
        }
        total = total
            .checked_add(bytes.len())
            .ok_or("pack byte size overflow")?;
        if total > MAX_PACK_BYTES {
            return Err(format!(
                "{name:?}: pack plaintext exceeds 6 MiB by {} bytes",
                total - MAX_PACK_BYTES
            ));
        }
        if sha256(bytes) != entry.sha256 {
            return Err(format!("{name:?}: SHA-256 mismatch"));
        }
        stickers.push(bytes);
    }
    if let Some(extra) = files
        .keys()
        .find(|name| **name != "manifest.json" && !names.contains(**name))
    {
        return Err(format!("{extra:?}: file not listed in manifest"));
    }
    Ok(Files {
        pack_id: sha256(manifest),
        stickers,
    })
}

#[cfg(test)]
mod tests;
