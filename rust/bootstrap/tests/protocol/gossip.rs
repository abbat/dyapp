use super::*;

#[tokio::test]
async fn rejected_gossip_bans_and_drops_its_source() {
    let address = start_config(|c| {
        c.roles = vec![Role::Store, Role::Search];
        c.limits.strikes_to_ban = 1;
        c.limits.min_free_mb = 0;
    })
    .await;
    let mut client = Client::connect_with_search(&address, Keypair::generate_ed25519(), true).await;
    let topic = dyapp_p2p_net::search_topic(0).hash();
    client
        .wait(|event| match event {
            BehaviourEvent::Gossip(libp2p::gossipsub::Event::Subscribed {
                topic: subscribed,
                ..
            }) if subscribed == topic => Some(()),
            _ => None,
        })
        .await;
    client
        .swarm
        .behaviour_mut()
        .gossip
        .as_mut()
        .unwrap()
        .publish(topic, b"junk".to_vec())
        .unwrap();
    let server = client.server;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let SwarmEvent::ConnectionClosed { peer_id, .. } =
                client.swarm.select_next_some().await
            {
                if peer_id == server {
                    break;
                }
            }
        }
    })
    .await
    .expect("the node kept the connection of a rejected gossip source");
}
