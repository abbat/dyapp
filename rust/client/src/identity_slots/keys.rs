use super::{Slot, Storage};
use dyapp_identity::Identity;
use std::fs;
use std::io::Read;
use zeroize::Zeroizing;

pub(super) const MAX_WRAPPED_BYTES: usize = 4096;

/// Platform-only reading of direct credentials or decoding of wrapped identity bytes.
pub trait KeyReader {
    /// Whether credentials exist for this slot, including an orphan legacy wrapping key.
    ///
    /// # Errors
    /// Returns platform key-store access failures; an error is never confirmed absence.
    fn contains(&self, slot: &Slot) -> Result<bool, String>;

    /// Open a system credential (None) or decode the supplied wrapped file (Some).
    /// Returns None only when the platform confirms the credential is missing.
    ///
    /// # Errors
    /// Returns denied access or decoding failures, without generating any replacement key.
    fn open(&self, slot: &Slot, wrapped: Option<&[u8]>) -> Result<Option<Identity>, String>;
}

pub(super) fn load(slot: &Slot, keys: &dyn KeyReader) -> Result<Option<Identity>, String> {
    let storage = slot.storage.ok_or("Invalid slot storage metadata")?;
    if storage == Storage::System {
        return keys.open(slot, None);
    }
    let path = slot.dir.join("identity.key");
    if storage == Storage::Wrapped {
        return match wrapped_bytes(&path)? {
            Some(bytes) => keys.open(slot, Some(&bytes)),
            None => Ok(None),
        };
    }
    let metadata = match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        result => result.map_err(|error| error.to_string())?,
    };
    if !metadata.is_file() {
        return Err("Identity path is not a regular file".into());
    }
    private_file(&metadata)?;
    let limit = 33;
    let mut bytes = Zeroizing::new(Vec::new());
    fs::File::open(path)
        .and_then(|file| file.take(limit).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    let secret =
        <&[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| "Invalid identity file length")?;
    Ok(Some(Identity::from_secret(secret)))
}

pub(super) fn wrapped_bytes(path: &std::path::Path) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        result => result.map_err(|error| error.to_string())?,
    };
    if !metadata.is_file() {
        return Err("Wrapped key path is not a regular file".into());
    }
    let mut bytes = Zeroizing::new(Vec::new());
    fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_WRAPPED_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_WRAPPED_BYTES {
        return Err("Wrapped key exceeds 4096 bytes".into());
    }
    Ok(Some(bytes))
}

fn private_file(metadata: &fs::Metadata) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("Identity file is accessible to other users".into());
        }
    }
    #[cfg(not(unix))]
    let _ = metadata;
    Ok(())
}
