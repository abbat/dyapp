//! Signed heartbeat time against the node's wall clock: the skew window edges, and wall-clock
//! jumps that leave monotonic time where it was.
use super::*;
use crate::clock::Manual;

const T: u64 = 1_800_000_000;

/// A service on a manual clock and one profile owner.
struct Node {
    service: Service,
    identity: Identity,
}

impl Node {
    fn new(manual: &Arc<Manual>) -> Self {
        let dir = format!("/tmp/ai/test-service-{}", uuid::Uuid::new_v4());
        let mut config = NodeConfig::default();
        config.storage.dir = dir.clone().into();
        config.network.id_pow_bits = 0;
        config.search.min_profile_pow_bits = 0;
        let store = BootstrapStore::new(&dir).unwrap();
        Self {
            service: Service::with_clock(store, config, manual.clock()),
            identity: Identity::generate(),
        }
    }

    fn send(&self, request: ProfileRequest) -> ProfileResponse {
        self.service.profile(&peer("p"), request).unwrap()
    }

    fn beat(&self, time: u64) -> Status {
        status(&self.beat_reply(time))
    }

    fn beat_reply(&self, time: u64) -> ProfileResponse {
        let beat = proto::Heartbeat {
            time,
            ..Default::default()
        };
        self.send(ProfileRequest {
            request: Some(profile_request::Request::Heartbeat(
                self.identity.sign(Domain::Heartbeat, beat.encode_to_vec()),
            )),
            age_seconds: 0,
        })
    }

    /// The beat is refused with the node's time as the hint.
    fn invalid(&self, time: u64, server_time: u64) {
        let response = self.beat_reply(time);
        assert_eq!(status(&response), Status::Invalid, "beat at {time}");
        assert_eq!(response.server_time, server_time, "hint for beat at {time}");
    }

    fn seen(&self) -> u64 {
        let path = self.service.config.storage.dir.join("profiles.db");
        rusqlite::Connection::open(path)
            .unwrap()
            .query_row(
                "SELECT `last_seen` FROM `profiles` WHERE `peer_id` = ?",
                [self.identity.peer_id()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            .unsigned_abs()
    }
}

#[test]
fn heartbeat_skew_window_follows_the_node_clock() {
    let manual = Manual::at(T);
    let node = Node::new(&manual);
    let profile = Profile {
        version: 1,
        ..Profile::default()
    };
    assert_eq!(status(&node.send(publish(profile.sign(&node.identity)))), Status::Ok);
    // A fresh publish is seen at the node's time, not the host's.
    assert_eq!(node.seen(), T);

    // The window is inclusive at CLOCK_SKEW and stores the signed time.
    assert_eq!(node.beat(T - CLOCK_SKEW), Status::Ok);
    assert_eq!(node.seen(), T);
    assert_eq!(node.beat(T + CLOCK_SKEW), Status::Ok);
    assert_eq!(node.seen(), T + CLOCK_SKEW);
    node.invalid(T + CLOCK_SKEW + 1, T);
    node.invalid(T - CLOCK_SKEW - 1, T);

    // The wall clock steps two hours ahead: the old window closes, the new one opens.
    manual.set_unix(T + 7200);
    node.invalid(T + CLOCK_SKEW, T + 7200);
    assert_eq!(node.beat(T + 7200 - CLOCK_SKEW), Status::Ok);
    assert_eq!(node.seen(), T + 7200 - CLOCK_SKEW);

    // And two hours behind the start: hints follow, stored time never moves back.
    manual.set_unix(T - 7200);
    node.invalid(T, T - 7200);
    assert_eq!(node.beat(T - 7200), Status::Ok);
    assert_eq!(node.seen(), T + 7200 - CLOCK_SKEW);
}
