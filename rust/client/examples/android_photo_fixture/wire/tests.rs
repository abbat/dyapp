use super::*;
use std::io::Cursor;

#[test]
fn reduced_proof_policy_is_confined_to_literal_loopback_tcp() {
    for address in [
        "/ip4/127.0.0.1/tcp/33445",
        "/ip4/127.0.0.2/tcp/1",
        "/ip6/::1/tcp/65535",
    ] {
        assert!(loopback(address).is_ok(), "{address}");
    }
    for address in [
        "/ip4/192.0.2.1/tcp/33445",
        "/ip4/0.0.0.0/tcp/1",
        "/ip6/::/tcp/1",
        "/ip6/2001:db8::1/tcp/1",
        "/dns4/localhost/tcp/33445",
        "/ip4/127.0.0.1/udp/33445/quic-v1",
        "/ip4/127.0.0.1/tcp/0",
        "/ip4/127.0.0.1/tcp/33445/p2p-circuit",
        "/ip4/127.0.0.1",
    ] {
        assert!(loopback(address).is_err(), "{address}");
    }
}

#[test]
fn controller_requires_complete_bounded_known_commands() {
    let mut input = Cursor::new(b"{\"op\":\"receive\"}\n{\"op\":\"quit\"}\n");
    assert!(matches!(
        read_from(&mut input).unwrap(),
        Some(Command::Receive {})
    ));
    assert!(matches!(
        read_from(&mut input).unwrap(),
        Some(Command::Quit {})
    ));
    assert!(read_from(&mut input).unwrap().is_none());
    for invalid in [
        b"{\"op\":\"receive\"}".as_slice(),
        b"{\"op\":\"delete\"}\n",
        b"{\"op\":\"quit\",\"typo\":1}\n",
    ] {
        assert!(read_from(Cursor::new(invalid)).is_err(), "{invalid:?}");
    }
    let oversized = vec![b' '; usize::try_from(MAX_COMMAND + 1).unwrap()];
    assert!(read_from(Cursor::new(oversized)).is_err());
}

#[test]
fn byte_and_peer_bounds_are_explicit_without_platform_decoding() {
    let signature = b"\x89PNG\r\n\x1a\n";
    assert!(png(signature).is_ok()); // Pixel validity remains the receiver's responsibility.
    assert!(png(b"JPEG").is_err());
    let mut oversized = vec![0; MAX_PNG + 1];
    oversized[..signature.len()].copy_from_slice(signature);
    assert!(png(&oversized).is_err());
    assert!(peer_id(&"a1".repeat(32)).is_ok());
    for invalid in [
        "A".repeat(64),
        "g".repeat(64),
        "0".repeat(63),
        "0".repeat(65),
    ] {
        assert!(peer_id(&invalid).is_err());
    }
}

#[test]
fn committed_queued_ids_remain_public_without_capabilities_or_error_payloads() {
    for accepted in [false, true] {
        let submission = if accepted {
            Submission::Accepted { id: [7; 16] }
        } else {
            Submission::Queued {
                id: [7; 16],
                error: dyapp_client::session::Error::Invalidated,
            }
        };
        assert_eq!(
            submitted(&submission),
            serde_json::json!({"id": vec![7; 16], "accepted": accepted})
        );
    }
}
