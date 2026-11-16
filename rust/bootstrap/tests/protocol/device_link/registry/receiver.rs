use super::*;
use dyapp_client::identity_slots::{LinkPhase, LinkReceiver, ProfileView, ReceivedLink};
use libp2p::futures::FutureExt as _;

struct Fixture {
    registry: Registry,
    receiver: LinkReceiver<String, String>,
    view: ProfileView,
}

impl Fixture {
    fn new(address: &Multiaddr) -> Self {
        let registry = Registry::new(protocol_build_path("link-receiver"), Layout::Desktop);
        let (source, owner, _) = registry.create(Storage::File, &Files).unwrap();
        network::save(
            &source.dir,
            Settings::Active,
            &Network {
                name: String::new(),
                seeds: vec![address.to_string()],
            },
        )
        .unwrap();
        let view = ProfileView::default();
        let receiver = {
            let guard = view.current().unwrap();
            let invitation = registry
                .begin_link(
                    &source.id,
                    &owner.peer_id(),
                    false,
                    &Files,
                    dyapp_client::now(),
                )
                .unwrap();
            LinkReceiver::new(invitation, guard.generation()).unwrap()
        };
        Self {
            registry,
            receiver,
            view,
        }
    }
}

async fn connect(address: Multiaddr) -> Result<dyapp_client::Node, String> {
    dyapp_client::Node::connect(address, Arc::new(dyapp_p2p_net::Protocols::public()))
        .await
        .map_err(|error| error.to_string())
}

fn install_id(received: &mut ReceivedLink) -> Result<String, String> {
    received
        .install_with(&Files, &persist_device)
        .map(|(slot, _, _)| slot.id.clone())
        .map_err(ToString::to_string)
}

#[tokio::test]
async fn empty_poll_restores_the_invitation_and_success_never_repeats_network_or_writes() {
    let address = link_node().await;
    let fixture = Fixture::new(&address);
    let receiver = &fixture.receiver;
    let code = receiver.code();
    let now = dyapp_client::now();
    let phase = receiver
        .poll_with(
            &Files,
            &fixture.view,
            now,
            |_| async {
                assert_eq!(receiver.result().unwrap(), None);
                connect(address.clone()).await
            },
            |_| panic!("an empty mailbox cannot install"),
        )
        .await
        .unwrap();
    assert_eq!(phase, LinkPhase::Waiting);
    assert_eq!(receiver.code(), code);
    assert_eq!(fixture.registry.list().unwrap().len(), 1);
    let mut sender = connect(address.clone()).await.unwrap();
    code.send(&mut sender, &Identity::generate(), b"private state", now)
        .await
        .unwrap();
    assert_eq!(
        receiver
            .poll_with(&Files, &fixture.view, now, |_| connect(address), install_id)
            .await
            .unwrap(),
        LinkPhase::Completed
    );
    let result = receiver.result().unwrap().unwrap().unwrap();
    assert_eq!(fixture.registry.list().unwrap().len(), 2);
    assert_eq!(
        receiver
            .poll_with(
                &Files,
                &fixture.view,
                now,
                |_| async { panic!("terminal receiver dialed again") },
                |_| panic!("terminal receiver wrote again")
            )
            .await
            .unwrap(),
        LinkPhase::Completed
    );
    assert_eq!(receiver.result().unwrap(), Some(Ok(result)));
}

#[tokio::test]
async fn cancellation_during_receive_retains_the_record_without_installing_it() {
    let address = link_node().await;
    let fixture = Fixture::new(&address);
    let receiver = &fixture.receiver;
    let now = dyapp_client::now();
    let mut sender = connect(address.clone()).await.unwrap();
    receiver
        .code()
        .send(&mut sender, &Identity::generate(), b"cancelled state", now)
        .await
        .unwrap();
    let result = receiver
        .poll_with(
            &Files,
            &fixture.view,
            now,
            |_| async {
                assert_eq!(receiver.result().unwrap(), None);
                assert_eq!(receiver.cancel(), LinkPhase::Cancelled);
                connect(address).await
            },
            |_| panic!("a cancelled receive installed"),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(receiver.phase(), LinkPhase::Cancelled);
    assert_eq!(receiver.result().unwrap(), None);
    assert_eq!(fixture.registry.list().unwrap().len(), 1);
}

#[tokio::test]
async fn a_callback_unwind_retains_interrupted_state_instead_of_retrying() {
    let address = link_node().await;
    let fixture = Fixture::new(&address);
    let receiver = &fixture.receiver;
    let now = dyapp_client::now();
    let mut sender = connect(address.clone()).await.unwrap();
    receiver
        .code()
        .send(
            &mut sender,
            &Identity::generate(),
            b"interrupted state",
            now,
        )
        .await
        .unwrap();
    let receive = receiver.poll_with(
        &Files,
        &fixture.view,
        now,
        |_| connect(address),
        |received| {
            received
                .install_with(&Files, &|_, _| panic!("controlled platform unwind"))
                .map(|(slot, _, _)| slot.id.clone())
                .map_err(ToString::to_string)
        },
    );
    assert!(std::panic::AssertUnwindSafe(receive)
        .catch_unwind()
        .await
        .is_err());
    assert_eq!(receiver.phase(), LinkPhase::Interrupted);
    assert!(receiver.result().is_err());
    assert_eq!(fixture.registry.list().unwrap().len(), 2);
    assert_eq!(
        receiver
            .poll_with(
                &Files,
                &fixture.view,
                now,
                |_| async { panic!("interrupted receiver dialed again") },
                |_| panic!("interrupted receiver retried installation")
            )
            .await
            .unwrap(),
        LinkPhase::Interrupted
    );
    assert_eq!(fixture.registry.list().unwrap().len(), 2);
}

#[tokio::test]
async fn a_writer_failure_is_cached_without_a_second_destination_or_callback() {
    let address = link_node().await;
    let fixture = Fixture::new(&address);
    let receiver = &fixture.receiver;
    let now = dyapp_client::now();
    let mut sender = connect(address.clone()).await.unwrap();
    receiver
        .code()
        .send(&mut sender, &Identity::generate(), b"failed state", now)
        .await
        .unwrap();
    let phase = receiver
        .poll_with(
            &Files,
            &fixture.view,
            now,
            |_| connect(address),
            |received| {
                received
                    .install_with(&Files, &|_, _| Err("platform unavailable".into()))
                    .map(|(slot, _, _)| slot.id.clone())
                    .map_err(ToString::to_string)
            },
        )
        .await
        .unwrap();
    assert_eq!(phase, LinkPhase::Failed);
    assert_eq!(
        receiver.result().unwrap(),
        Some(Err("platform unavailable".into()))
    );
    assert_eq!(fixture.registry.list().unwrap().len(), 2);
    assert_eq!(
        receiver
            .poll_with(
                &Files,
                &fixture.view,
                now,
                |_| async { panic!("failed receiver dialed again") },
                |_| panic!("failed receiver retried installation")
            )
            .await
            .unwrap(),
        LinkPhase::Failed
    );
    assert_eq!(fixture.registry.list().unwrap().len(), 2);
}
