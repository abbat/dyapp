use thiserror::Error;

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Replication error: {0}")]
    ReplicationError(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Message not found")]
    MessageNotFound,

    #[error("Profile not found")]
    ProfileNotFound,

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error(transparent)]
    Profile(#[from] dyapp_profile::Error),
}

pub type Result<T> = std::result::Result<T, BootstrapError>;
