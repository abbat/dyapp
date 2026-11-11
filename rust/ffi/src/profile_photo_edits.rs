//! Snapshot-bound editor actions; clients carry opaque revisions instead of inferring identity.
use crate::profile_draft::database;
use crate::{CoreError, IdentityKey};
use dyapp_client::store::profile_photos::{self, PhotoEdit};
use dyapp_client::store::{Error, Store};
use dyapp_profile::Profile;
use std::sync::Arc;

/// One authoritative photo order and the opaque identity of all its original/preview hashes.
#[derive(uniffi::Record)]
pub struct ProfilePhotoSnapshot {
    /// Pass unchanged with an action from this displayed snapshot.
    pub revision: Vec<u8>,
    /// Thumbnail hashes in display order, including duplicates.
    pub thumbnails: Vec<Vec<u8>>,
}

/// An action on positions in a displayed photo snapshot.
#[derive(uniffi::Enum)]
pub enum ProfilePhotoEdit {
    /// Remove the displayed position.
    Remove {
        /// Position in the expected snapshot.
        index: u32,
    },
    /// Move between displayed positions.
    Move {
        /// Source position in the expected snapshot.
        from: u32,
        /// Destination position in the expected snapshot.
        to: u32,
    },
}

/// Stable refusal of a stale photo action, distinct from storage failures.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ProfilePhotoEditError {
    /// Reload the displayed snapshot; never retry the old numerical action automatically.
    #[error("profile photos changed")]
    Changed,
    /// An invalid action or a storage failure.
    #[error("{reason}")]
    Failed {
        /// Diagnostic detail; not primary user-facing copy.
        reason: String,
    },
}

fn snapshot(profile: &Profile) -> ProfilePhotoSnapshot {
    ProfilePhotoSnapshot {
        revision: profile_photos::revision(profile).to_vec(),
        thumbnails: profile
            .photos
            .iter()
            .map(|photo| photo.thumbnail.clone())
            .collect(),
    }
}

/// Read photo order and revision from one draft read.
///
/// # Errors
/// Returns identity-directory, database or draft decoding failures.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
pub fn profile_photo_snapshot(
    dir: String,
    key: Arc<IdentityKey>,
) -> Result<ProfilePhotoSnapshot, CoreError> {
    let store = Store::open(&database(&dir)?.join("client.db"), &key.identity)
        .map_err(|error| CoreError::Failed(error.to_string()))?;
    let profile = store
        .profile_draft()
        .map_err(|error| CoreError::Failed(error.to_string()))?
        .unwrap_or_default();
    Ok(snapshot(&profile))
}

/// Verify the displayed revision and edit under the same core writer transaction.
///
/// # Errors
/// Returns Changed before mutation when any original, preview or order differs; otherwise
/// returns invalid-position or persistence failure. The returned snapshot is the committed result.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
pub fn edit_profile_photos(
    dir: String,
    key: Arc<IdentityKey>,
    expected_revision: Vec<u8>,
    edit: ProfilePhotoEdit,
) -> Result<ProfilePhotoSnapshot, ProfilePhotoEditError> {
    let path = database(&dir)
        .map_err(|error| ProfilePhotoEditError::Failed {
            reason: error.to_string(),
        })?
        .join("client.db");
    let mut store = Store::open(&path, &key.identity).map_err(code)?;
    let position = |value| usize::try_from(value).unwrap_or(usize::MAX);
    let edit = match edit {
        ProfilePhotoEdit::Remove { index } => PhotoEdit::Remove(position(index)),
        ProfilePhotoEdit::Move { from, to } => PhotoEdit::Move {
            from: position(from),
            to: position(to),
        },
    };
    store
        .edit_profile_photos(&expected_revision, edit)
        .map(|profile| snapshot(&profile))
        .map_err(code)
}

#[cfg(test)]
#[path = "profile_photo_edits_tests.rs"]
mod tests;

fn code(error: Error) -> ProfilePhotoEditError {
    match error {
        Error::PhotoChanged => ProfilePhotoEditError::Changed,
        Error::PhotoRefused => ProfilePhotoEditError::Failed {
            reason: "photoRefused".into(),
        },
        error => ProfilePhotoEditError::Failed {
            reason: error.to_string(),
        },
    }
}
