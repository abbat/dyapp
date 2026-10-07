pub mod codec;
pub mod encryption;
pub mod ice;
pub mod session;

pub use codec::{CodecFormat, CodecInfo};
pub use encryption::FrameEncryption;
pub use ice::{ICECandidate, ICEGatheringState};
pub use session::{SessionState, VideoSession};

#[derive(Debug, Clone)]
pub enum VideoError {
    SessionNotInitialized,
    OfferGenerationFailed(String),
    AnswerGenerationFailed(String),
    ICEError(String),
    EncryptionError(String),
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

pub type Result<T> = std::result::Result<T, VideoError>;
