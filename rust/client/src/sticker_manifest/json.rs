//! Typed, bounded JSON decoding bound to the original manifest bytes.

use super::{File, Files, MAX_MANIFEST_BYTES, MAX_STICKERS};
use dyapp_profile::is_clean_text;
use serde::Deserialize;
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Deserialize)]
struct Version {
    format: u64,
}

#[derive(Deserialize)]
struct Raw {
    format: u64,
    name: String,
    stickers: Vec<RawSticker>,
}

#[derive(Deserialize)]
struct RawSticker {
    file: String,
    sha256: String,
    emoji: String,
}

/// A manifest entry whose hash and display label have been validated.
pub struct Sticker {
    /// Exact root filename; membership is checked by `Parsed::check_files`.
    pub file: String,
    /// Parsed SHA-256 digest.
    pub sha256: [u8; 32],
    /// One clean extended grapheme, preserved without case conversion.
    pub emoji: String,
}

/// A parsed manifest retaining the original bytes as identity authority.
pub struct Parsed<'a> {
    original: &'a [u8],
    name: String,
    stickers: Vec<Sticker>,
}

impl Parsed<'_> {
    /// Clean, nonblank pack name, at most 64 code points.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Entries in manifest order.
    pub fn stickers(&self) -> &[Sticker] {
        &self.stickers
    }

    /// Binds the parsed entries to exact files and hashes. Still does not validate pixels.
    pub fn check_files<'a>(&self, files: &BTreeMap<&str, &'a [u8]>) -> Result<Files<'a>, String> {
        let entries: Vec<_> = self
            .stickers
            .iter()
            .map(|sticker| File {
                name: &sticker.file,
                sha256: sticker.sha256,
            })
            .collect();
        super::check_files(self.original, &entries, files)
    }
}

pub(crate) fn clean_label(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= 64 && is_clean_text(text)
}

pub(crate) fn valid_emoji(text: &str) -> bool {
    clean_label(text) && text.graphemes(true).count() == 1
}

fn hash(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut hash = [0; 32];
    for (out, pair) in hash.iter_mut().zip(text.as_bytes().as_chunks::<2>().0) {
        let nibble = |byte: u8| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => 0, // All bytes were checked above.
        };
        *out = (nibble(pair[0]) << 4) | nibble(pair[1]);
    }
    Some(hash)
}

/// Decodes at most 64 KiB before file/pixel checks. Unknown fields are ignored for
/// future extensions; duplicate known fields are refused. Unknown format asks to update.
pub fn parse(bytes: &[u8]) -> Result<Parsed<'_>, String> {
    if bytes.is_empty() || bytes.len() > MAX_MANIFEST_BYTES {
        return Err("manifest.json: invalid manifest size".into());
    }
    let version: Version =
        serde_json::from_slice(bytes).map_err(|_| "manifest.json: invalid JSON format")?;
    if version.format != 1 {
        return Err("manifest.json: update the app for this format".into());
    }
    let raw: Raw =
        serde_json::from_slice(bytes).map_err(|_| "manifest.json: invalid or duplicate fields")?;
    if raw.format != 1 || !clean_label(&raw.name) {
        return Err("manifest.json: invalid pack name".into());
    }
    if raw.stickers.is_empty() || raw.stickers.len() > MAX_STICKERS {
        return Err("manifest.json: sticker count must be 1..=120".into());
    }
    let stickers = raw
        .stickers
        .into_iter()
        .map(|raw| {
            if !super::safe_file(&raw.file) {
                return Err(format!("{:?}: unsafe root filename", raw.file));
            }
            if !valid_emoji(&raw.emoji) {
                return Err(format!("{:?}: label must be one clean grapheme", raw.file));
            }
            let sha256 =
                hash(&raw.sha256).ok_or_else(|| format!("{:?}: invalid SHA-256", raw.file))?;
            Ok(Sticker {
                file: raw.file,
                sha256,
                emoji: raw.emoji,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(Parsed {
        original: bytes,
        name: raw.name,
        stickers,
    })
}

#[cfg(test)]
mod tests;
