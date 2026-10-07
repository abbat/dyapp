use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ICEGatheringState {
    New,
    Gathering,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ICECandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub timestamp: i64,
}

impl ICECandidate {
    pub fn new(candidate: String, sdp_mid: Option<String>, sdp_mline_index: Option<u16>) -> Self {
        Self {
            candidate,
            sdp_mid,
            sdp_mline_index,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
        }
    }

    pub fn is_relay(&self) -> bool {
        self.candidate.contains("relay")
    }

    pub fn is_host(&self) -> bool {
        self.candidate.contains("host")
    }

    pub fn is_srflx(&self) -> bool {
        self.candidate.contains("srflx")
    }

    pub fn priority(&self) -> u32 {
        if self.is_host() {
            300
        } else if self.is_srflx() {
            200
        } else if self.is_relay() {
            100
        } else {
            0
        }
    }
}

#[derive(Debug)]
pub struct ICEGathering {
    state: ICEGatheringState,
    candidates: Vec<ICECandidate>,
}

impl ICEGathering {
    pub fn new() -> Self {
        Self {
            state: ICEGatheringState::New,
            candidates: Vec::new(),
        }
    }

    pub fn start_gathering(&mut self) {
        self.state = ICEGatheringState::Gathering;
    }

    pub fn add_candidate(&mut self, candidate: ICECandidate) {
        self.candidates.push(candidate);
    }

    pub fn complete_gathering(&mut self) {
        self.state = ICEGatheringState::Complete;
    }

    pub fn state(&self) -> ICEGatheringState {
        self.state
    }

    pub fn candidates(&self) -> &[ICECandidate] {
        &self.candidates
    }

    pub fn best_candidate(&self) -> Option<&ICECandidate> {
        self.candidates.iter().max_by_key(|c| c.priority())
    }
}

impl Default for ICEGathering {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ice_candidate_creation() {
        let cand = ICECandidate::new(
            "candidate:123 1 udp 1234 1.2.3.4 5000 typ host".to_string(),
            None,
            None,
        );
        assert!(cand.is_host());
        assert!(!cand.is_relay());
    }

    #[test]
    fn test_ice_candidate_priority() {
        let host = ICECandidate::new(
            "candidate:123 1 udp 1234 1.2.3.4 5000 typ host".to_string(),
            None,
            None,
        );
        let srflx = ICECandidate::new(
            "candidate:123 1 udp 1234 1.2.3.4 5000 typ srflx".to_string(),
            None,
            None,
        );
        let relay = ICECandidate::new(
            "candidate:123 1 udp 1234 1.2.3.4 5000 typ relay".to_string(),
            None,
            None,
        );

        assert!(host.priority() > srflx.priority());
        assert!(srflx.priority() > relay.priority());
    }

    #[test]
    fn test_ice_gathering() {
        let mut gathering = ICEGathering::new();
        assert_eq!(gathering.state(), ICEGatheringState::New);

        gathering.start_gathering();
        assert_eq!(gathering.state(), ICEGatheringState::Gathering);

        let cand = ICECandidate::new("candidate:123".to_string(), None, None);
        gathering.add_candidate(cand);
        assert_eq!(gathering.candidates().len(), 1);

        gathering.complete_gathering();
        assert_eq!(gathering.state(), ICEGatheringState::Complete);
    }

    #[test]
    fn test_best_candidate_selection() {
        let mut gathering = ICEGathering::new();
        gathering.start_gathering();

        let relay = ICECandidate::new("candidate:1 typ relay".to_string(), None, None);
        let host = ICECandidate::new("candidate:2 typ host".to_string(), None, None);
        let srflx = ICECandidate::new("candidate:3 typ srflx".to_string(), None, None);

        gathering.add_candidate(relay);
        gathering.add_candidate(srflx);
        gathering.add_candidate(host);

        let best = gathering.best_candidate().unwrap();
        assert!(best.is_host());
    }
}
