use dyapp_video::VideoSession as RustVideoSession;
use std::sync::Arc;
use tokio::sync::RwLock;
use uniffi::*;

// Public FFI interface for Swift/Kotlin
// Minimal surface: peer operations, key management, message/video

#[derive(Clone, uniffi::Object)]
pub struct PeerConnection {
    id: String,
}

#[uniffi::export]
impl PeerConnection {
    #[uniffi::constructor]
    pub fn new(peer_id: String) -> Arc<Self> {
        Arc::new(Self { id: peer_id })
    }

    pub fn get_id(&self) -> String {
        self.id.clone()
    }
}

#[derive(Clone, uniffi::Object)]
pub struct MessageService {}

#[uniffi::export]
impl MessageService {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    pub fn send_message(&self, peer_id: String, text: String) -> Result<String, String> {
        let _ = text;
        Ok(format!("Message queued to {}", peer_id))
    }

    pub fn queue_size(&self) -> i32 {
        0
    }
}

#[derive(Clone, uniffi::Object)]
pub struct VideoSession {
    inner: Arc<RwLock<Option<RustVideoSession>>>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[uniffi::export]
impl VideoSession {
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, String> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| format!("Failed to create runtime: {}", e))?;

        Ok(Arc::new(Self {
            inner: Arc::new(RwLock::new(None)),
            runtime: Arc::new(runtime),
        }))
    }

    pub fn create_offer(&self) -> Result<String, String> {
        self.runtime.block_on(async {
            let mut session_guard = self.inner.write().await;
            if session_guard.is_none() {
                *session_guard = Some(
                    RustVideoSession::new()
                        .await
                        .map_err(|error| error.to_string())?,
                );
            }

            if let Some(session) = session_guard.as_ref() {
                session.create_offer().await.map_err(|e| format!("{:?}", e))
            } else {
                Err("Failed to create session".to_string())
            }
        })
    }

    pub fn receive_answer(&self, sdp: String) -> Result<(), String> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                session
                    .receive_answer(sdp)
                    .await
                    .map_err(|e| format!("{:?}", e))
            } else {
                Err("Session not initialized".to_string())
            }
        })
    }

    pub fn mark_offer_sent(&self) -> Result<(), String> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                session
                    .mark_offer_sent()
                    .await
                    .map_err(|e| format!("{:?}", e))
            } else {
                Err("Session not initialized".to_string())
            }
        })
    }

    pub fn close(&self) -> Result<(), String> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                session.close().await.map_err(|e| format!("{:?}", e))
            } else {
                Err("Session not initialized".to_string())
            }
        })
    }

    pub fn get_session_id(&self) -> Result<String, String> {
        self.runtime.block_on(async {
            let session_guard = self.inner.read().await;
            if let Some(session) = session_guard.as_ref() {
                Ok(session.session_id().to_string())
            } else {
                Err("Session not initialized".to_string())
            }
        })
    }
}

#[uniffi::export]
pub fn generate_keypair() -> Result<KeyPair, String> {
    Ok(KeyPair {
        public_key: "placeholder".to_string(),
        secret_key: "placeholder".to_string(),
    })
}

#[derive(Clone, uniffi::Record)]
pub struct KeyPair {
    pub public_key: String,
    pub secret_key: String,
}

#[derive(Clone, uniffi::Object)]
pub struct ProfileService {}

#[uniffi::export]
impl ProfileService {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    pub fn get_profile(&self) -> Result<UserProfile, String> {
        Ok(UserProfile {
            user_id: "placeholder".to_string(),
            age: 0,
            verified: false,
        })
    }

    pub fn update_field(&self, field: String, value: String) -> Result<(), String> {
        let _ = (field, value);
        Ok(())
    }
}

#[derive(Clone, uniffi::Record)]
pub struct UserProfile {
    pub user_id: String,
    pub age: i32,
    pub verified: bool,
}

// Generated UniFFI scaffolding: a doc-comment gap and large metadata const arrays.
mod scaffolding {
    #![allow(clippy::empty_line_after_doc_comments, clippy::large_const_arrays)]
    include!(concat!(env!("OUT_DIR"), "/dyapp.uniffi.rs"));
}
pub use scaffolding::*;
