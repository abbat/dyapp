mod slots;

use super::*;
use dyapp_p2p_net::{build_configured_swarm, Mode, SwarmOptions};
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use std::net::TcpListener;

fn temp_dir(tag: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/native-network-tests")
        .join(tag)
        .join(crate::IdentityKey::generate().peer_id());
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_str().unwrap().into()
}

async fn serve_node(sender: std::sync::mpsc::Sender<(u16, String)>) {
    let options = SwarmOptions {
        mode: Mode::Auto,
        ..SwarmOptions::default()
    };
    let mut swarm = build_configured_swarm(Keypair::generate_ed25519(), options).unwrap();
    let peer = swarm.local_peer_id().to_string();
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    loop {
        let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await else {
            continue;
        };
        if let Some(Protocol::Tcp(port)) = address.iter().nth(1) {
            sender.send((port, peer.clone())).unwrap();
        }
    }
}

/// A public-network node on a background thread; returns its TCP port and peer ID.
pub(crate) fn node() -> (u16, String) {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(serve_node(sender));
    });
    receiver.recv_timeout(Duration::from_secs(5)).unwrap()
}

// Seed rows dial both TCP and QUIC; reserve both ports so another test cannot answer either.
pub(crate) fn reserved_endpoint() -> (TcpListener, std::net::UdpSocket) {
    for _ in 0..8 {
        let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
        match std::net::UdpSocket::bind(tcp.local_addr().unwrap()) {
            Ok(udp) => return (tcp, udp),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
            Err(error) => panic!("reserve UDP fixture: {error}"),
        }
    }
    panic!("Could not reserve a TCP/QUIC fixture endpoint");
}

#[test]
fn a_checked_draft_is_saved_and_only_a_connected_one_activates() {
    let dir = temp_dir("check");
    let check = ConnectionCheck::new(dir.clone()).unwrap();
    assert_eq!(load_network(dir.clone(), NetworkFile::Draft), Ok(None));
    assert_eq!(check.activate(), Err(CheckError::NotChecked));

    // Keep the bound listener alive until the check completes; its port cannot be reused.
    let (listener, _udp) = reserved_endpoint();
    let closed = listener.local_addr().unwrap().port();
    let reject = reject_handshake(listener.try_clone().unwrap());
    let unreachable = NetworkSettings {
        seeds: vec![format!("127.0.0.1:{closed}"), "seed".into()],
        name: String::new(),
    };
    let rows = check.run(unreachable.clone(), 5000).unwrap();
    reject.join().unwrap();
    assert!(matches!(rows[0], SeedRow::Unreachable { .. }), "{rows:?}");
    assert_eq!(rows[1], SeedRow::InvalidAddress);
    assert_eq!(
        load_network(dir.clone(), NetworkFile::Draft),
        Ok(Some(unreachable))
    );
    assert_eq!(check.activate(), Err(CheckError::NotChecked));

    invalid_drafts(&check);
    connected_draft(&dir, &check);
}

fn invalid_drafts(check: &ConnectionCheck) {
    let empty = NetworkSettings {
        seeds: vec![],
        name: String::new(),
    };
    assert_eq!(check.run(empty, 2000), Err(CheckError::Empty));
    let bad_name = NetworkSettings {
        seeds: vec!["seed:1".into()],
        name: " city".into(),
    };
    assert!(matches!(
        check.run(bad_name, 2000),
        Err(CheckError::InvalidNetwork { .. })
    ));
}

fn connected_draft(dir: &str, check: &ConnectionCheck) {
    let (port, peer) = node();
    let reachable = NetworkSettings {
        seeds: vec![format!("127.0.0.1:{port}/{peer}")],
        name: String::new(),
    };
    let rows = check.run(reachable.clone(), 5000).unwrap();
    assert_eq!(rows, vec![SeedRow::Connected { peer_id: peer }]);
    assert_eq!(load_network(dir.to_owned(), NetworkFile::Active), Ok(None));
    assert_eq!(check.activate(), Ok(reachable.clone()));
    assert_eq!(
        load_network(dir.to_owned(), NetworkFile::Active),
        Ok(Some(reachable))
    );
}

pub(crate) fn reject_handshake(listener: TcpListener) -> std::thread::JoinHandle<()> {
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let (stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
                .await
                .unwrap()
                .unwrap();
            drop(stream);
        });
    })
}

#[test]
fn cancel_stops_a_running_check() {
    let check = ConnectionCheck::new(temp_dir("cancel")).unwrap();
    let worker = check.clone();
    let (mute, _udp) = reserved_endpoint();
    let silent = NetworkSettings {
        seeds: vec![mute.local_addr().unwrap().to_string()],
        name: String::new(),
    };
    let (entered, ready) = std::sync::mpsc::channel();
    let (finished, completed) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let result = worker.run_with_ready(silent, 60_000, || entered.send(()).unwrap());
        drop(worker);
        finished.send(result).unwrap();
    });
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    check.cancel();
    assert_eq!(
        completed.recv_timeout(Duration::from_secs(5)).unwrap(),
        Err(CheckError::Cancelled)
    );
    thread.join().unwrap();
}
