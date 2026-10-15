//! Prototype video sessions, codec descriptions and local ICE bookkeeping.

/// Codec descriptions and local format matching.
pub mod codec;
/// Prototype frame encryption helpers.
pub mod encryption;
/// Local ICE candidate bookkeeping.
pub mod ice;
/// Caller-side WebRTC session setup.
pub mod session;

pub use codec::{CodecFormat, CodecInfo};
pub use encryption::FrameEncryption;
pub use ice::{ICECandidate, ICEGatheringState};
pub use session::{SessionState, VideoSession};

#[derive(Debug, Clone)]
/// Errors reported by video helpers and sessions.
pub enum VideoError {
    /// The requested session has not been initialized.
    SessionNotInitialized,
    /// Offer creation failed with the supplied details.
    OfferGenerationFailed(String),
    /// Answer creation failure; currently unused.
    AnswerGenerationFailed(String),
    /// The peer connection rejected an ICE operation.
    ICEError(String),
    /// Frame encryption failure; currently unused.
    EncryptionError(String),
    /// Invalid session state or peer connection operation, with details.
    InvalidState(String),
}

impl std::fmt::Display for VideoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionNotInitialized => write!(f, "Video session not initialized"),
            Self::OfferGenerationFailed(e) => write!(f, "Offer generation failed: {}", e),
            Self::AnswerGenerationFailed(e) => write!(f, "Answer generation failed: {}", e),
            Self::ICEError(e) => write!(f, "ICE error: {}", e),
            Self::EncryptionError(e) => write!(f, "Encryption error: {}", e),
            Self::InvalidState(e) => write!(f, "Invalid state: {}", e),
        }
    }
}

impl std::error::Error for VideoError {}

/// A video operation result.
pub type Result<T> = std::result::Result<T, VideoError>;
