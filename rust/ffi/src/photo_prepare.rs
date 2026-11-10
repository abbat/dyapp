//! Common `UniFFI` conversion layer for shared photo preparation.

use dyapp_client::photo::prepare;

/// Which shared photo preparation policy applies to an original.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PhotoPurpose {
    /// Public profile original.
    Profile,
    /// Chat attachment original.
    Chat,
}

impl From<PhotoPurpose> for prepare::Purpose {
    fn from(value: PhotoPurpose) -> Self {
        match value {
            PhotoPurpose::Profile => Self::Profile,
            PhotoPurpose::Chat => Self::Chat,
        }
    }
}

/// Typed safe failure from the shared preparation core.
#[derive(Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum PhotoPreparationError {
    /// Source or decoded pixels exceed the applicable budget, or no candidate fits.
    #[error("tooLarge")]
    TooLarge,
    /// Unsupported source, malformed pixels or an encoding failure.
    #[error("unsupported")]
    Unsupported,
}

impl From<prepare::PrepareError> for PhotoPreparationError {
    fn from(error: prepare::PrepareError) -> Self {
        match error {
            prepare::PrepareError::TooLarge => Self::TooLarge,
            prepare::PrepareError::Unsupported => Self::Unsupported,
        }
    }
}

/// Platform decoder dimensions selected by the shared core.
#[derive(Debug, PartialEq, Eq, uniffi::Record)]
pub struct PhotoDecodePlan {
    /// Upright target width.
    pub width: u32,
    /// Upright target height.
    pub height: u32,
}

/// Prepared original and thumbnail, with the original's actual dimensions.
#[derive(Debug, uniffi::Record)]
pub struct PreparedPhotoData {
    /// Metadata-free JPEG original.
    pub full: Vec<u8>,
    /// Shared bounded JPEG preview.
    pub thumbnail: Vec<u8>,
    /// Original width after fitting.
    pub width: u32,
    /// Original height after fitting.
    pub height: u32,
}

/// The shared core's picked-file limit for bounded platform reading.
#[uniffi::export]
#[must_use]
pub fn photo_picked_byte_limit() -> u64 {
    prepare::MAX_PICKED_BYTES
}

/// Validate a source header and select dimensions before the platform allocates decoded pixels.
///
/// # Errors
/// Returns the core's typed source-format or size rejection.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned native strings"
)]
pub fn photo_decode_plan(
    mime: String,
    width: u32,
    height: u32,
    bytes: u64,
    purpose: PhotoPurpose,
) -> Result<PhotoDecodePlan, PhotoPreparationError> {
    prepare::decode_plan(&mime, width, height, bytes, purpose.into())
        .map(|plan| PhotoDecodePlan {
            width: plan.width,
            height: plan.height,
        })
        .map_err(PhotoPreparationError::from)
}

/// Prepare upright straight RGBA within the core's decode plan, without native-side encoding policy.
///
/// # Errors
/// Returns the core's typed pixel, size or encoding rejection.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned native byte buffers"
)]
pub fn prepare_photo_pixels(
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    purpose: PhotoPurpose,
) -> Result<PreparedPhotoData, PhotoPreparationError> {
    prepare::prepare_photo(&rgba, width, height, purpose.into())
        .map(|photo| PreparedPhotoData {
            full: photo.full,
            thumbnail: photo.thumbnail,
            width: photo.width,
            height: photo.height,
        })
        .map_err(PhotoPreparationError::from)
}

#[cfg(test)]
mod tests;
