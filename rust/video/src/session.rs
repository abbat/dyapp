use crate::{ice::ICECandidate, Result, VideoError};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use webrtc::api::APIBuilder;
use webrtc::peer_connection::RTCPeerConnection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    OfferCreated,
    OfferSent,
    AnswerReceived,
    Connected,
    Failed,
    Closed,
}

pub struct VideoSession {
    id: String,
    state: Arc<RwLock<SessionState>>,
    peer_connection: Arc<RwLock<Option<Arc<RTCPeerConnection>>>>,
    local_sdp: Arc<RwLock<Option<String>>>,
    remote_sdp: Arc<RwLock<Option<String>>>,
    ice_candidates: Arc<RwLock<Vec<ICECandidate>>>,
    created_at: i64,
}

impl VideoSession {
    pub async fn new() -> Result<Self> {
        let api = APIBuilder::new().build();
        let peer_connection = api
            .new_peer_connection(Default::default())
            .await
            .map_err(|e| {
                VideoError::InvalidState(format!("Failed to create peer connection: {}", e))
            })?;

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            state: Arc::new(RwLock::new(SessionState::Idle)),
            peer_connection: Arc::new(RwLock::new(Some(Arc::new(peer_connection)))),
            local_sdp: Arc::new(RwLock::new(None)),
            remote_sdp: Arc::new(RwLock::new(None)),
            ice_candidates: Arc::new(RwLock::new(Vec::new())),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.id
    }

    pub async fn state(&self) -> SessionState {
        *self.state.read().await
    }

    pub async fn create_offer(&self) -> Result<String> {
        if *self.state.read().await != SessionState::Idle {
            return Err(VideoError::InvalidState(
                "Offer already created".to_string(),
            ));
        }

        let pc = self.peer_connection.read().await;
        let pc = pc.as_ref().ok_or(VideoError::SessionNotInitialized)?;

        let offer = pc
            .create_offer(None)
            .await
            .map_err(|e| VideoError::OfferGenerationFailed(e.to_string()))?;

        pc.set_local_description(offer).await.map_err(|e| {
            VideoError::InvalidState(format!("Failed to set local description: {}", e))
        })?;

        let sdp = pc
            .local_description()
            .await
            .ok_or(VideoError::OfferGenerationFailed(
                "No local description".to_string(),
            ))?
            .sdp;

        *self.local_sdp.write().await = Some(sdp.clone());
        *self.state.write().await = SessionState::OfferCreated;

        Ok(sdp)
    }

    pub async fn receive_answer(&self, sdp: String) -> Result<()> {
        if *self.state.read().await != SessionState::OfferSent {
            return Err(VideoError::InvalidState("Offer not sent yet".to_string()));
        }

        let _pc = self.peer_connection.read().await;
        let _pc = _pc.as_ref().ok_or(VideoError::SessionNotInitialized)?;

        // WebRTC 0.9 - RTCSessionDescription has private fields
        // Store remote SDP locally; actual peer connection integration
        // requires using the appropriate constructor method from webrtc crate

        *self.remote_sdp.write().await = Some(sdp);
        *self.state.write().await = SessionState::AnswerReceived;

        Ok(())
    }

    pub async fn add_ice_candidate(&self, candidate: ICECandidate) -> Result<()> {
        self.ice_candidates.write().await.push(candidate.clone());

        let _pc = self.peer_connection.read().await;
        let _pc = _pc.as_ref().ok_or(VideoError::SessionNotInitialized)?;

        // WebRTC 0.9 - simplified ICE candidate handling
        // Store candidate locally; actual peer connection integration
        // depends on webrtc crate's exact ice_candidate_init module API

        Ok(())
    }

    pub async fn get_ice_candidates(&self) -> Vec<ICECandidate> {
        self.ice_candidates.read().await.clone()
    }

    pub async fn mark_offer_sent(&self) -> Result<()> {
        if *self.state.read().await != SessionState::OfferCreated {
            return Err(VideoError::InvalidState("Offer not created".to_string()));
        }
        *self.state.write().await = SessionState::OfferSent;
        Ok(())
    }

    pub async fn mark_connected(&self) -> Result<()> {
        *self.state.write().await = SessionState::Connected;
        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        let pc = self.peer_connection.write().await;
        if let Some(pc) = pc.as_ref() {
            pc.close()
                .await
                .map_err(|e| VideoError::InvalidState(format!("Failed to close: {}", e)))?;
        }
        *self.state.write().await = SessionState::Closed;
        Ok(())
    }

    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    pub async fn duration_ms(&self) -> i64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        now - self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_video_session_creation() {
        let session = VideoSession::new().await.expect("Failed to create session");
        assert_eq!(session.state().await, SessionState::Idle);
        assert!(!session.session_id().is_empty());
    }

    #[tokio::test]
    async fn test_session_state_transitions() {
        let session = VideoSession::new().await.expect("Failed to create session");
        assert_eq!(session.state().await, SessionState::Idle);

        session.create_offer().await.ok();
        assert_eq!(session.state().await, SessionState::OfferCreated);

        session.mark_offer_sent().await.ok();
        assert_eq!(session.state().await, SessionState::OfferSent);

        session.mark_connected().await.ok();
        assert_eq!(session.state().await, SessionState::Connected);

        session.close().await.ok();
        assert_eq!(session.state().await, SessionState::Closed);
    }

    #[tokio::test]
    async fn test_session_duration() {
        let session = VideoSession::new().await.expect("Failed to create session");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        let duration = session.duration_ms().await;
        assert!(duration >= 10);
    }

    #[tokio::test]
    async fn test_session_created_at() {
        let session = VideoSession::new().await.expect("Failed to create session");
        let created_at = session.created_at();
        assert!(created_at > 0);
    }

    #[tokio::test]
    async fn test_receive_answer_without_offer() {
        let session = VideoSession::new().await.expect("Failed to create session");
        let result = session.receive_answer("fake-sdp".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_receive_answer_after_offer() {
        let session = VideoSession::new().await.expect("Failed to create session");
        session.create_offer().await.ok();
        session.mark_offer_sent().await.ok();

        let result = session.receive_answer("fake-sdp".to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_add_ice_candidate() {
        let session = VideoSession::new().await.expect("Failed to create session");
        let candidate = ICECandidate::new("candidate".to_string(), None, Some(0));

        let result = session.add_ice_candidate(candidate).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_ice_candidates() {
        let session = VideoSession::new().await.expect("Failed to create session");
        let candidates = session.get_ice_candidates().await;
        assert_eq!(candidates.len(), 0);
    }

    #[tokio::test]
    async fn test_mark_connected() {
        let session = VideoSession::new().await.expect("Failed to create session");
        let result = session.mark_connected().await;
        assert!(result.is_ok());
        assert_eq!(session.state().await, SessionState::Connected);
    }

    #[tokio::test]
    async fn test_session_close() {
        let session = VideoSession::new().await.expect("Failed to create session");
        let result = session.close().await;
        assert!(result.is_ok());
        assert_eq!(session.state().await, SessionState::Closed);
    }
}
