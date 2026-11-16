use super::*;
use dyapp_client::identity_slots::ProfileView;

struct Source {
    registry: Registry,
    slot: Slot,
    owner: Identity,
    network: Network,
    state: Zeroizing<Vec<u8>>,
}

impl Source {
    fn new(address: &Multiaddr) -> Self {
        let registry = Registry::new(protocol_build_path("link-owner-source"), Layout::Desktop);
        let (slot, owner, _) = registry.create(Storage::File, &Files).unwrap();
        let store = Store::open(&slot.dir.join("client.db"), &owner).unwrap();
        store.set_profile_hidden(true).unwrap();
        let state = Zeroizing::new(store.link_state().unwrap());
        drop(store);
        let port = address
            .iter()
            .find_map(|part| match part {
                Protocol::Tcp(port) => Some(port),
                _ => None,
            })
            .unwrap();
        let network = Network {
            name: String::new(),
            seeds: vec![format!("127.0.0.1:{port}")],
        };
        network::save(&slot.dir, Settings::Active, &network).unwrap();
        Self {
            registry,
            slot,
            owner,
            network,
            state,
        }
    }
}

#[tokio::test]
async fn confirmed_owner_transfer_delivers_the_same_identity_and_complete_private_state() {
    let address = link_node().await;
    let source = Source::new(&address);
    let now = dyapp_client::now();
    let device = Identity::generate();
    let mut invitation = Invite::new(dyapp_client::mailbox(&device.public_key()), now).unwrap();
    let view = ProfileView::default();
    let sender = source
        .registry
        .prepare_link_sender(
            &view,
            &source.slot.id,
            &source.owner.peer_id(),
            &invitation.code().text(),
            &Files,
            now,
        )
        .unwrap();
    assert_eq!(sender.code(), *invitation.code());
    let destination = address.clone();
    let expected = &source.network;
    sender
        .confirm_and_send_with(&Files, &view, now, |actual| async move {
            assert_eq!(&actual, expected);
            dyapp_client::Node::connect(destination, Arc::new(dyapp_p2p_net::Protocols::public()))
                .await
                .map_err(|error| error.to_string())
        })
        .await
        .unwrap();
    let mut node =
        dyapp_client::Node::connect(address, Arc::new(dyapp_p2p_net::Protocols::public()))
            .await
            .unwrap();
    let received = invitation
        .receive(&mut node, &device, now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received.identity.peer_id(), source.owner.peer_id());
    let state = Zeroizing::new(received.state);
    assert_eq!(state.as_slice(), source.state.as_slice());
    assert_eq!(source.registry.list().unwrap(), vec![source.slot]);
}
