use crate::{ice::ICECandidate, Result, VideoError};
use rtc::ice::mdns::MulticastDnsMode;
use rtc::rtp_transceiver::rtp_sender::RtpCodecKind;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use webrtc::peer_connection::{
    MediaEngine, PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCIceCandidateInit, RTCSessionDescription, SettingEngineBuilder,
};

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

// webrtc requires an event handler; no connection events are consumed yet.
struct NoEvents;

impl PeerConnectionEventHandler for NoEvents {}

async fn new_peer_connection() -> webrtc::error::Result<impl PeerConnection> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;
    // No mDNS: our peers send IP host candidates, and the multicast join fails
    // without a network interface (ENODEV). Remote `.local` candidates are not resolved.
    let setting_engine = SettingEngineBuilder::new()
        .with_multicast_dns_mode(MulticastDnsMode::Disabled)
        .build();
    PeerConnectionBuilder::new()
        .with_media_engine(media_engine)
        .with_setting_engine(setting_engine)
        .with_handler(Arc::new(NoEvents))
        .with_udp_addrs(vec!["0.0.0.0:0"])
        .build()
        .await
}

pub struct VideoSession {
    id: String,
    state: Arc<RwLock<SessionState>>,
    peer_connection: Arc<dyn PeerConnection>,
    local_sdp: Arc<RwLock<Option<String>>>,
    remote_sdp: Arc<RwLock<Option<String>>>,
    ice_candidates: Arc<RwLock<Vec<ICECandidate>>>,
    created_at: i64,
}

impl VideoSession {
    pub async fn new() -> Result<Self> {
        let peer_connection = new_peer_connection().await.map_err(|e| {
            VideoError::InvalidState(format!("Failed to create peer connection: {}", e))
        })?;
        // Without a media section the offer carries no ICE credentials and a peer rejects it.
        peer_connection
            .add_transceiver_from_kind(RtpCodecKind::Video, None)
            .await
            .map_err(|e| VideoError::InvalidState(format!("Failed to add video: {}", e)))?;

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            state: Arc::new(RwLock::new(SessionState::Idle)),
            peer_connection: Arc::new(peer_connection),
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

        let pc = &self.peer_connection;
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

        let answer = RTCSessionDescription::answer(sdp.clone())
            .map_err(|e| VideoError::InvalidState(format!("Invalid answer: {}", e)))?;
        self.peer_connection
            .set_remote_description(answer)
            .await
            .map_err(|e| {
                VideoError::InvalidState(format!("Failed to set remote description: {}", e))
            })?;

        *self.remote_sdp.write().await = Some(sdp);
        *self.state.write().await = SessionState::AnswerReceived;

        // Candidates that arrived before the answer were only stored.
        for candidate in self.ice_candidates.read().await.iter() {
            self.apply_ice_candidate(candidate).await?;
        }

        Ok(())
    }

    /// Stores a remote candidate and hands it to the peer connection once the answer is set;
    /// earlier candidates are applied by `receive_answer`.
    pub async fn add_ice_candidate(&self, candidate: ICECandidate) -> Result<()> {
        let mut candidates = self.ice_candidates.write().await;
        if self.remote_sdp.read().await.is_some() {
            self.apply_ice_candidate(&candidate).await?;
        }
        candidates.push(candidate);
        Ok(())
    }

    async fn apply_ice_candidate(&self, candidate: &ICECandidate) -> Result<()> {
        self.peer_connection
            .add_ice_candidate(RTCIceCandidateInit {
                candidate: candidate.candidate.clone(),
                sdp_mid: candidate.sdp_mid.clone(),
                sdp_mline_index: candidate.sdp_mline_index,
                ..Default::default()
            })
            .await
            .map_err(|e| VideoError::ICEError(e.to_string()))
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
        self.peer_connection
            .close()
            .await
            .map_err(|e| VideoError::InvalidState(format!("Failed to close: {}", e)))?;
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

    /// Answers `offer` from a second, independent peer connection.
    async fn remote_answer(offer: String) -> String {
        let remote = new_peer_connection().await.expect("remote peer");
        remote
            .set_remote_description(RTCSessionDescription::offer(offer).expect("offer"))
            .await
            .expect("set offer");
        let answer = remote.create_answer(None).await.expect("answer");
        remote
            .set_local_description(answer.clone())
            .await
            .expect("set answer");
        answer.sdp
    }

    #[tokio::test]
    async fn test_receive_answer_after_offer() {
        let session = VideoSession::new().await.expect("Failed to create session");
        let offer = session.create_offer().await.expect("offer");
        assert!(offer.starts_with("v=0"));
        session.mark_offer_sent().await.ok();

        // A candidate that arrives before the answer is applied together with it.
        let candidate = ICECandidate::new(
            "candidate:1 1 udp 2130706431 127.0.0.1 50000 typ host".to_string(),
            None,
            Some(0),
        );
        session.add_ice_candidate(candidate).await.expect("stored");

        let answer = remote_answer(offer).await;
        session
            .receive_answer(answer.clone())
            .await
            .expect("answer applied");
        assert_eq!(session.state().await, SessionState::AnswerReceived);
        assert_eq!(*session.remote_sdp.read().await, Some(answer));
    }

    #[tokio::test]
    async fn test_receive_answer_rejects_invalid_sdp() {
        let session = VideoSession::new().await.expect("Failed to create session");
        session.create_offer().await.expect("offer");
        session.mark_offer_sent().await.ok();

        let result = session.receive_answer("fake-sdp".to_string()).await;
        assert!(result.is_err());
        assert_eq!(session.state().await, SessionState::OfferSent);
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
