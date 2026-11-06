//! Immutable sticker pack records, validated identically for TAR import and peer exchange.

use crate::attachment::sealed_size;
use crate::sticker_manifest::{json, MAX_MANIFEST_BYTES, MAX_STICKERS, MAX_STICKER_BYTES};
use crate::{sticker_tar, sticker_webp};
use dyapp_identity::sha256;
use prost::encoding::{decode_key, decode_varint, skip_field, DecodeContext, WireType};
use prost::Message;
use std::collections::BTreeMap;

const MAX_SEALED_BYTES: usize = 6 * 1024 * 1024;

#[derive(Clone, PartialEq, Message)]
struct Record {
    #[prost(bytes = "vec", tag = "1")]
    manifest: Vec<u8>,
    #[prost(bytes = "vec", repeated, tag = "2")]
    files: Vec<Vec<u8>>,
}

/// A validated, immutable pack. RGBA frames are checked one at a time, not retained.
pub struct Pack {
    id: [u8; 32],
    name: String,
    stickers: Vec<json::Sticker>,
    record: Record,
}

impl Pack {
    /// Original manifest digest, independent of protobuf encoding or TAR metadata.
    pub fn id(&self) -> &[u8; 32] {
        &self.id
    }
    /// Display-safe pack name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Manifest order and validated labels/hashes.
    pub fn stickers(&self) -> &[json::Sticker] {
        &self.stickers
    }
    /// Original compressed bytes for storage/upload; use the core decoder for display.
    /// OS decoders must not receive these bytes; future playback needs further validation.
    pub fn sticker_bytes(&self, index: usize) -> Option<&[u8]> {
        self.record.files.get(index).map(Vec::as_slice)
    }
    /// Protobuf attachment plaintext; the sealed blob fits the 6 MiB limit.
    pub fn encode(&self) -> Vec<u8> {
        self.record.encode_to_vec()
    }
}

/// Imports a bounded TAR without opening any archive path on disk.
pub fn import_tar(bytes: &[u8]) -> Result<Pack, String> {
    let files = sticker_tar::read(bytes)?;
    let manifest = files
        .get("manifest.json")
        .copied()
        .ok_or("manifest.json: missing file")?;
    let parsed = json::parse(manifest)?;
    let checked = parsed.check_files(&files)?;
    let record = Record {
        manifest: manifest.to_vec(),
        files: checked.stickers.into_iter().map(<[u8]>::to_vec).collect(),
    };
    validate(record, None)
}

/// Opens a pack for the requested manifest ID. Peer input never reaches the TAR reader.
pub fn from_peer(bytes: &[u8], expected: &[u8; 32]) -> Result<Pack, String> {
    preflight(bytes)?;
    let record = Record::decode(bytes).map_err(|_| "invalid sticker pack protobuf")?;
    validate(record, Some(expected))
}

fn check_size(plaintext: usize) -> Result<(), String> {
    let sealed = sealed_size(plaintext).ok_or("sticker pack size overflow")?;
    if sealed > MAX_SEALED_BYTES {
        return Err(format!(
            "sealed sticker pack exceeds 6 MiB by {} bytes",
            sealed - MAX_SEALED_BYTES
        ));
    }
    Ok(())
}

// Count and bound fields before prost allocates any Vec for peer-controlled repetition.
fn preflight(mut bytes: &[u8]) -> Result<(), String> {
    check_size(bytes.len())?;
    let mut manifest = false;
    let mut count = 0;
    while !bytes.is_empty() {
        let (tag, wire) = decode_key(&mut bytes).map_err(|_| "invalid sticker pack field")?;
        if tag != 1 && tag != 2 {
            skip_field(wire, tag, &mut bytes, DecodeContext::default())
                .map_err(|_| "invalid sticker pack extension")?;
            continue;
        }
        if wire != WireType::LengthDelimited {
            return Err("invalid sticker pack field type".into());
        }
        let size =
            usize::try_from(decode_varint(&mut bytes).map_err(|_| "invalid sticker pack length")?)
                .map_err(|_| "sticker pack length overflow")?;
        if tag == 1 {
            if manifest || size == 0 || size > MAX_MANIFEST_BYTES {
                return Err("manifest.json: duplicate or invalid byte size".into());
            }
            manifest = true;
        } else {
            count += 1;
            if count > MAX_STICKERS || size == 0 || size > MAX_STICKER_BYTES {
                return Err("sticker pack file count or byte limit exceeded".into());
            }
        }
        bytes = bytes.get(size..).ok_or("truncated sticker pack field")?;
    }
    if !manifest {
        return Err("manifest.json: missing field".into());
    }
    Ok(())
}

fn validate(record: Record, expected: Option<&[u8; 32]>) -> Result<Pack, String> {
    check_size(record.encoded_len())?;
    let id = sha256(&record.manifest);
    if expected.is_some_and(|expected| *expected != id) {
        return Err("sticker pack ID mismatch".into());
    }
    let parsed = json::parse(&record.manifest)?;
    if record.files.len() < parsed.stickers().len() {
        return Err(format!(
            "{:?}: missing peer file",
            parsed.stickers()[record.files.len()].file
        ));
    }
    if record.files.len() != parsed.stickers().len() {
        return Err("manifest.json: extra peer files".into());
    }
    let mut files = BTreeMap::from([("manifest.json", record.manifest.as_slice())]);
    for (entry, bytes) in parsed.stickers().iter().zip(&record.files) {
        files.insert(entry.file.as_str(), bytes.as_slice());
    }
    parsed.check_files(&files)?;
    for (entry, bytes) in parsed.stickers().iter().zip(&record.files) {
        sticker_webp::decode::first_frame(bytes)
            .map_err(|reason| format!("{:?}: {reason}", entry.file))?;
    }
    let name = parsed.name().to_owned();
    let stickers = parsed
        .stickers()
        .iter()
        .map(|entry| json::Sticker {
            file: entry.file.clone(),
            sha256: entry.sha256,
            emoji: entry.emoji.clone(),
        })
        .collect();
    Ok(Pack {
        id,
        name,
        stickers,
        record,
    })
}

#[cfg(test)]
mod tests;
