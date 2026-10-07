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

impl axum::response::IntoResponse for BootstrapError {
    fn into_response(self) -> axum::response::Response {
        use axum::http::StatusCode;
        let status = match &self {
            Self::MessageNotFound | Self::ProfileNotFound => StatusCode::NOT_FOUND,
            Self::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            Self::Profile(dyapp_profile::Error::Stale) => StatusCode::CONFLICT,
            Self::Profile(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            axum::Json(serde_json::json!({ "error": self.to_string() })),
        )
            .into_response()
    }
}

pub type Result<T> = std::result::Result<T, BootstrapError>;
