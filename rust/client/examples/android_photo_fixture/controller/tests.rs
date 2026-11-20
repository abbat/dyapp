use super::*;

fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let client =
        TcpStream::connect_timeout(&listener.local_addr().unwrap(), Duration::from_secs(5))
            .unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    client
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    (client, listener.accept().unwrap().0)
}

#[tokio::test(flavor = "current_thread")]
async fn socket_commands_complete_and_public_replies_are_framed() {
    let (mut guest, server) = pair();
    let mut controller = Controller::from_socket(server).unwrap();
    guest
        .write_all(b"{\"op\":\"receive\"}\n{\"op\":\"quit\"}\n")
        .unwrap();
    assert!(matches!(
        controller.read().await.unwrap(),
        Some(Command::Receive {})
    ));
    controller
        .reply(&serde_json::json!({"open": true}))
        .unwrap();
    let mut reply = String::new();
    std::io::BufRead::read_line(&mut BufReader::new(guest.try_clone().unwrap()), &mut reply)
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reply).unwrap(),
        serde_json::json!({"open": true})
    );
    assert!(matches!(
        controller.read().await.unwrap(),
        Some(Command::Quit {})
    ));
    drop(guest);
    assert!(controller.read().await.unwrap().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn initialization_admits_only_a_valid_public_owner_id() {
    for valid in [false, true] {
        let (mut guest, server) = pair();
        let mut controller = Controller::from_socket(server).unwrap();
        let peer = if valid {
            "a".repeat(64)
        } else {
            "secret".to_owned()
        };
        wire::write_to(&mut guest, &serde_json::json!({"op": "init", "peer": peer})).unwrap();
        let initialized = controller.initialize().await;
        assert_eq!(initialized.is_ok(), valid);
        if valid {
            assert_eq!(initialized.unwrap(), peer);
        }
    }
    let (mut guest, server) = pair();
    let mut controller = Controller::from_socket(server).unwrap();
    guest.write_all(b"{\"op\":\"receive\"}\n").unwrap();
    assert!(controller.initialize().await.is_err());
}

#[test]
fn expired_frame_deadline_refuses_even_newly_available_bytes() {
    let (mut guest, server) = pair();
    guest.write_all(b"x").unwrap();
    let mut stream = DeadlineStream {
        stream: server,
        deadline: Instant::now(),
    };
    let failure = stream.read(&mut [0]).unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::TimedOut);
}

#[test]
fn a_new_command_gets_a_fresh_deadline() {
    let (mut guest, server) = pair();
    let stream = DeadlineStream {
        stream: server,
        deadline: Instant::now(),
    };
    let mut source = Source::Socket(BufReader::new(stream));
    guest.write_all(b"{\"op\":\"text\"}\n").unwrap();
    assert!(matches!(source.command().unwrap(), Some(Command::Text {})));
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_guest_input_fails_instead_of_becoming_eof_or_success() {
    let (mut guest, server) = pair();
    let controller = Controller::from_socket(server).unwrap();
    guest
        .write_all(b"{\"op\":\"receive\",\"typo\":true}\n")
        .unwrap();
    assert!(controller.read().await.is_err());
}
