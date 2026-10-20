//! Native exports: identity keys and MLS encryption; messaging, profiles and video are
//! prototypes with explicit stubs.

use dyapp_video::VideoSession as RustVideoSession;
use std::sync::Arc;
use tokio::sync::RwLock;
use uniffi::*;
use zeroize::Zeroizing;

mod mls;
pub use mls::{MlsDevice, MlsGroup, MlsNewGroup, MlsReceived};

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

    /// Applies the remote SDP answer, then queued ICE candidates, to the connection.
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

/// A failed [`KeyStore`] call. Unlike the flat `CoreError`, it can be thrown by the app.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum WrapError {
    /// The keystore's reason, for logs; it must not contain key material.
    #[error("{reason}")]
    Failed {
        /// The keystore's reason.
        reason: String,
    },
}

/// The app threw something other than a `WrapError`.
impl From<uniffi::UnexpectedUniFFICallbackError> for WrapError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed {
            reason: error.reason,
        }
    }
}

impl From<WrapError> for CoreError {
    fn from(error: WrapError) -> Self {
        Self::Failed(error.to_string())
    }
}

/// The platform keystore (Android Keystore, Keychain, Secret Service, DPAPI), implemented by
/// the app: it encrypts with a key that never leaves the keystore.
#[uniffi::export(with_foreign)]
pub trait KeyStore: Send + Sync {
    /// Encrypts a secret key; the app persists the result.
    fn wrap(&self, secret: Vec<u8>) -> Result<Vec<u8>, WrapError>;
    /// Decrypts what `wrap` returned.
    fn unwrap(&self, wrapped: Vec<u8>) -> Result<Vec<u8>, WrapError>;
}

/// An Ed25519 identity or device key; its secret leaves Rust only through a [`KeyStore`].
#[derive(uniffi::Object)]
pub struct IdentityKey {
    identity: dyapp_identity::Identity,
}

#[uniffi::export]
impl IdentityKey {
    /// A new key from the OS random number generator.
    #[uniffi::constructor]
    pub fn generate() -> Arc<Self> {
        Arc::new(Self {
            identity: dyapp_identity::Identity::generate(),
        })
    }

    /// The key that `store` unwraps from `wrapped`, a blob from [`IdentityKey::wrap`].
    #[uniffi::constructor]
    pub fn unwrap(store: Arc<dyn KeyStore>, wrapped: Vec<u8>) -> Result<Arc<Self>, CoreError> {
        let secret = Zeroizing::new(store.unwrap(wrapped)?);
        let secret: &[u8; 32] = secret
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Failed("unwrapped key is not 32 bytes".to_string()))?;
        Ok(Arc::new(Self {
            identity: dyapp_identity::Identity::from_secret(secret),
        }))
    }

    /// The secret encrypted by `store`, for the app to persist.
    pub fn wrap(&self, store: Arc<dyn KeyStore>) -> Result<Vec<u8>, CoreError> {
        // ponytail: the FFI buffer that carries the secret to the app is freed, not wiped.
        Ok(store.wrap(self.identity.secret().to_vec())?)
    }

    /// Hex SHA-256 of the public key.
    pub fn peer_id(&self) -> String {
        self.identity.peer_id()
    }

    /// The Ed25519 public key bytes.
    pub fn public_key(&self) -> Vec<u8> {
        self.identity.public_key().to_vec()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// XOR with a fixed byte: stands in for the platform keystore.
    struct TestStore;

    impl KeyStore for TestStore {
        fn wrap(&self, secret: Vec<u8>) -> Result<Vec<u8>, WrapError> {
            Ok(secret.iter().map(|byte| byte ^ 0x5a).collect())
        }

        fn unwrap(&self, wrapped: Vec<u8>) -> Result<Vec<u8>, WrapError> {
            self.wrap(wrapped)
        }
    }

    /// A keystore whose key is gone.
    struct LostStore;

    impl KeyStore for LostStore {
        fn wrap(&self, _: Vec<u8>) -> Result<Vec<u8>, WrapError> {
            Err(WrapError::Failed {
                reason: "no key".to_string(),
            })
        }

        fn unwrap(&self, wrapped: Vec<u8>) -> Result<Vec<u8>, WrapError> {
            self.wrap(wrapped)
        }
    }

    #[test]
    fn keystore_errors_reach_the_caller() {
        let store: Arc<dyn KeyStore> = Arc::new(LostStore);
        let error = IdentityKey::generate().wrap(store.clone()).unwrap_err();
        assert_eq!(error.to_string(), "no key");
        assert!(IdentityKey::unwrap(store, vec![0; 32]).is_err());
    }

    #[test]
    fn wrapped_key_unwraps_to_the_same_peer_id() {
        let store: Arc<dyn KeyStore> = Arc::new(TestStore);
        let key = IdentityKey::generate();
        let wrapped = key.wrap(store.clone()).unwrap();
        assert_eq!(wrapped.len(), 32);
        assert_ne!(wrapped, key.identity.secret().to_vec());
        let opened = IdentityKey::unwrap(store, wrapped).unwrap();
        assert_eq!(opened.peer_id(), key.peer_id());
        assert_eq!(opened.public_key(), key.public_key());
    }

    #[test]
    fn unwrap_rejects_a_key_of_the_wrong_length() {
        let store: Arc<dyn KeyStore> = Arc::new(TestStore);
        for wrapped in [vec![], vec![0; 31], vec![0; 33]] {
            assert!(IdentityKey::unwrap(store.clone(), wrapped).is_err());
        }
    }
}

// Generated UniFFI scaffolding: a doc-comment gap and large metadata const arrays.
mod scaffolding {
    #![allow(clippy::empty_line_after_doc_comments, clippy::large_const_arrays)]
    include!(concat!(env!("OUT_DIR"), "/dyapp.uniffi.rs"));
}
pub use scaffolding::*;
