use thiserror::Error;

/// Errors of the node's stores and request handling.
#[derive(Debug, Error)]
pub enum BootstrapError {
    /// A database or file operation failed.
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Data could not be encoded or decoded.
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Erasure coding failed.
    #[error("Replication error: {0}")]
    ReplicationError(String),

    /// The peer exceeded its request rate.
    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    /// No such message.
    #[error("Message not found")]
    MessageNotFound,

    /// No such profile.
    #[error("Profile not found")]
    ProfileNotFound,

    /// The request is malformed.
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// An internal failure.
    #[error("Server error: {0}")]
    ServerError(String),

    /// The profile record was rejected.
    #[error(transparent)]
    Profile(#[from] dyapp_profile::Error),
}

/// Result of the node's stores and handlers.
pub type Result<T> = std::result::Result<T, BootstrapError>;
