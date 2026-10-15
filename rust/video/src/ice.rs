use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Manually maintained gathering state; independent of WebRTC events.
pub enum ICEGatheringState {
    /// Gathering has not been marked as started.
    New,
    /// Gathering was marked as started.
    Gathering,
    /// Gathering was marked as complete.
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// An unvalidated candidate string with optional SDP routing fields.
pub struct ICECandidate {
    /// Raw candidate string.
    pub candidate: String,
    /// Optional SDP media identifier.
    pub sdp_mid: Option<String>,
    /// Optional SDP media section index.
    pub sdp_mline_index: Option<u16>,
    /// Creation wall-clock time in milliseconds since the Unix epoch.
    pub timestamp: i64,
}

impl ICECandidate {
    /// Stores the fields without parsing and records the wall-clock time.
    ///
    /// # Panics
    /// Panics if the system clock precedes the Unix epoch.
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

    /// Tests for the literal substring `relay`, without parsing ICE syntax.
    pub fn is_relay(&self) -> bool {
        self.candidate.contains("relay")
    }

    /// Tests for the literal substring `host`, without parsing ICE syntax.
    pub fn is_host(&self) -> bool {
        self.candidate.contains("host")
    }

    /// Tests for the literal substring `srflx`, without parsing ICE syntax.
    pub fn is_srflx(&self) -> bool {
        self.candidate.contains("srflx")
    }

    /// Returns a local heuristic: host 300, srflx 200, relay 100, other 0.
    /// This is not the priority encoded in an ICE candidate.
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
/// An in-memory candidate list with manually assigned gathering state.
pub struct ICEGathering {
    state: ICEGatheringState,
    candidates: Vec<ICECandidate>,
}

impl ICEGathering {
    /// Creates an empty list in the new state.
    pub fn new() -> Self {
        Self {
            state: ICEGatheringState::New,
            candidates: Vec::new(),
        }
    }

    /// Marks gathering as started; does not perform network gathering.
    pub fn start_gathering(&mut self) {
        self.state = ICEGatheringState::Gathering;
    }

    /// Appends a candidate without validation, deduplication or state checks.
    pub fn add_candidate(&mut self, candidate: ICECandidate) {
        self.candidates.push(candidate);
    }

    /// Marks gathering as complete.
    pub fn complete_gathering(&mut self) {
        self.state = ICEGatheringState::Complete;
    }

    /// Returns the manually assigned state.
    pub fn state(&self) -> ICEGatheringState {
        self.state
    }

    /// Borrows all candidates in insertion order.
    pub fn candidates(&self) -> &[ICECandidate] {
        &self.candidates
    }

    /// Returns the last candidate with the highest local heuristic priority.
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
