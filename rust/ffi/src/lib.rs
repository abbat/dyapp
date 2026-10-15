//! Prototype native exports; messaging, keys and profiles include explicit stubs.

use dyapp_video::VideoSession as RustVideoSession;
use std::sync::Arc;
use tokio::sync::RwLock;
use uniffi::*;

// Public FFI interface for Swift/Kotlin
// Minimal surface: peer operations, key management, message/video

/// Caller-supplied peer label; this object does not open a network connection.
#[derive(Clone, uniffi::Object)]
pub struct PeerConnection {
    id: String,
}

#[uniffi::export]
impl PeerConnection {
    /// Stores the label without validating it as a peer ID.
    #[uniffi::constructor]
    pub fn new(peer_id: String) -> Arc<Self> {
        Arc::new(Self { id: peer_id })
    }

    /// Returns a copy of the stored label.
    pub fn get_id(&self) -> String {
        self.id.clone()
    }
}

/// Messaging placeholder; no queue, encryption or delivery is implemented here.
#[derive(Clone, uniffi::Object)]
pub struct MessageService {}

#[uniffi::export]
impl MessageService {
    /// Creates the stateless messaging placeholder.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    /// Ignores the text and returns a status string; nothing is queued or sent.
    pub fn send_message(&self, peer_id: String, text: String) -> Result<String, CoreError> {
        let _ = text;
        Ok(format!("Message queued to {}", peer_id))
    }

    /// Returns zero; there is no backing queue.
    pub fn queue_size(&self) -> i32 {
        0
    }
}

/// Lazy WebRTC session whose synchronous methods block on a private Tokio runtime.
#[derive(Clone, uniffi::Object)]
pub struct VideoSession {
    inner: Arc<RwLock<Option<RustVideoSession>>>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[uniffi::export]
impl VideoSession {
    /// Creates the private runtime; the WebRTC session is not initialized yet.
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, CoreError> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| CoreError::Failed(format!("Failed to create runtime: {}", e)))?;

        Ok(Arc::new(Self {
            inner: Arc::new(RwLock::new(None)),
            runtime: Arc::new(runtime),
        }))
    }

    /// Initializes the inner session if absent and returns its SDP offer.
    pub fn create_offer(&self) -> Result<String, CoreError> {
        self.runtime.block_on(async {
            let mut session_guard = self.inner.write().await;
            if session_guard.is_none() {
                *session_guard = Some(
                    RustVideoSession::new()
                        .await
                        .map_err(|error| CoreError::Failed(error.to_string()))?,
                );
            }

            if let Some(session) = session_guard.as_ref() {
                session
                    .create_offer()
                    .await
                    .map_err(|e| CoreError::Failed(format!("{:?}", e)))
            } else {
                Err(CoreError::Failed("Failed to create session".to_string()))
            }
        })
    }

    /// Stores an SDP answer in an initialized session; it is not applied to WebRTC.
    pub fn receive_answer(&self, sdp: String) -> Result<(), CoreError> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                session
                    .receive_answer(sdp)
                    .await
                    .map_err(|e| CoreError::Failed(format!("{:?}", e)))
            } else {
                Err(CoreError::Failed("Session not initialized".to_string()))
            }
        })
    }

    /// Advances the initialized session from offer-created to offer-sent.
    pub fn mark_offer_sent(&self) -> Result<(), CoreError> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                session
                    .mark_offer_sent()
                    .await
                    .map_err(|e| CoreError::Failed(format!("{:?}", e)))
            } else {
                Err(CoreError::Failed("Session not initialized".to_string()))
            }
        })
    }

    /// Closes the initialized inner session; the wrapper remains allocated.
    pub fn end_session(&self) -> Result<(), CoreError> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                session
                    .close()
                    .await
                    .map_err(|e| CoreError::Failed(format!("{:?}", e)))
            } else {
                Err(CoreError::Failed("Session not initialized".to_string()))
            }
        })
    }

    /// Returns the initialized inner session's UUID, or an initialization error.
    pub fn get_session_id(&self) -> Result<String, CoreError> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                Ok(session.session_id().to_string())
            } else {
                Err(CoreError::Failed("Session not initialized".to_string()))
            }
        })
    }
}

/// Error returned by every fallible export; the message may echo internal state.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum CoreError {
    /// Operation-specific failure text, without a stable machine-readable code.
    #[error("{0}")]
    Failed(String),
}

/// Peer ID of a freshly generated identity key; the key is dropped, nothing is stored.
#[uniffi::export]
pub fn generate_peer_id() -> String {
    dyapp_identity::Identity::generate().peer_id()
}

/// Returns placeholder key strings; no cryptographic key material is generated.
#[uniffi::export]
pub fn generate_keypair() -> Result<KeyPair, CoreError> {
    Ok(KeyPair {
        public_key: "placeholder".to_string(),
        secret_key: "placeholder".to_string(),
    })
}

/// Unvalidated key fields; the current generator fills both with placeholders.
#[derive(Clone, uniffi::Record)]
pub struct KeyPair {
    /// Public-key text; no encoding or validity is enforced.
    pub public_key: String,
    /// Secret-key text; the current generator does not supply a real secret.
    pub secret_key: String,
}

/// Profile placeholder; it does not read or persist user data.
#[derive(Clone, uniffi::Object)]
pub struct ProfileService {}

#[uniffi::export]
impl ProfileService {
    /// Creates the stateless profile placeholder.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    /// Returns the constant placeholder ID, age zero and unverified status.
    pub fn get_profile(&self) -> Result<UserProfile, CoreError> {
        Ok(UserProfile {
            user_id: "placeholder".to_string(),
            age: 0,
            verified: false,
        })
    }

    /// Accepts and discards both arguments; no profile field is changed.
    pub fn update_field(&self, field: String, value: String) -> Result<(), CoreError> {
        let _ = (field, value);
        Ok(())
    }
}

/// Native-facing profile fields, currently populated by the placeholder service.
#[derive(Clone, uniffi::Record)]
pub struct UserProfile {
    /// User label; the placeholder service returns `placeholder`.
    pub user_id: String,
    /// Age field; the placeholder service returns zero.
    pub age: i32,
    /// Verification flag; the placeholder service returns false.
    pub verified: bool,
}

// Generated UniFFI scaffolding: a doc-comment gap and large metadata const arrays.
mod scaffolding {
    #![allow(clippy::empty_line_after_doc_comments, clippy::large_const_arrays)]
    include!(concat!(env!("OUT_DIR"), "/dyapp.uniffi.rs"));
}
pub use scaffolding::*;
