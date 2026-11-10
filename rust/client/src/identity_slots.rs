//! Shared registry reading for the existing desktop and wrapped mobile layouts.

mod create;
mod write;
pub use create::KeyWriter;
pub use write::SelectionDurability;
mod inventory;
mod keys;
mod markers;
mod open;
mod registry;
pub use inventory::list;
pub use keys::KeyReader;
pub use markers::active;
pub use open::{open_slot, resolve, RecoveryReason, Resolution};
pub use registry::Registry;
use std::path::PathBuf;

/// Virtual slot for the pre-registry root identity.
pub const LEGACY: &str = "legacy-primary";

/// Existing on-disk formats; selecting one never migrates or writes files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Requires storage-mode markers and supports root system credentials.
    Desktop,
    /// Wrapped keys without storage-mode markers.
    Android,
    /// Wrapped keys plus sibling deletion markers and an explicit unselected pointer.
    Ios,
}

/// Lifecycle read from exact newline-terminated markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Creation has not been verified.
    Pending,
    /// Creation was verified; the key must still be opened before use.
    Ready,
    /// Confirmed removal has not finished.
    Deleting,
    /// Metadata is missing, malformed or not a regular file/directory.
    Broken,
}

/// Platform key-storage contract of a candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Storage {
    /// Direct system credential storage on desktop.
    System,
    /// Desktop local key file.
    File,
    /// Mobile key file wrapped by the platform key store.
    Wrapped,
}

/// One listed candidate; inventory does not read or unwrap its key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    /// Canonical UUID or the virtual legacy identifier.
    pub id: String,
    /// Existing directory containing this identity's data.
    pub dir: PathBuf,
    /// Lifecycle, including interrupted creation/removal.
    pub state: State,
    /// None when required storage metadata cannot be interpreted.
    pub storage: Option<Storage>,
}

/// The pointer's syntax, kept distinct from whether its target still exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Active {
    /// No pointer file.
    Absent,
    /// Malformed or unreadable metadata.
    Invalid,
    /// Explicit iOS selection clearing.
    Unselected,
    /// A syntactically valid candidate identifier.
    Selected(String),
}

pub(super) fn is_uuid(name: &str) -> bool {
    name.len() == 36
        && name.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
        })
}

#[cfg(test)]
mod tests;
