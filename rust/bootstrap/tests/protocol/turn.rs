//! TURN credentials over the wire through the client.
use super::*;

#[tokio::test]
async fn client_fetches_turn_credentials_only_from_a_turn_node() {
    dyapp_p2p_net::set_id_pow_bits(0);
    let protocols = std::sync::Arc::new(dyapp_p2p_net::Protocols::public());
    let store = start_config(|c| {
        c.roles = vec![Role::Store];
        c.limits.min_free_mb = 0;
    })
    .await;
    let mut node = dyapp_client::Node::connect(store, protocols.clone())
        .await
        .unwrap();
    assert!(matches!(
        node.turn().await,
        Err(dyapp_client::Error::Status(Status::Unsupported))
    ));

    let relay = start_config(|c| {
        c.roles = vec![Role::Store, Role::Turn];
        c.limits.min_free_mb = 0;
        c.turn.secret = "secret".into();
        c.turn.urls = vec!["turn:relay.example:3478".into()];
    })
    .await;
    let mut node = dyapp_client::Node::connect(relay, protocols).await.unwrap();
    let turn = node.turn().await.unwrap();
    assert_eq!(turn.urls, ["turn:relay.example:3478"]);
    let now = chrono::Utc::now().timestamp().unsigned_abs();
    assert!(turn.expires > now);
    assert!(turn.username.starts_with(&format!("{}:", turn.expires)));
    assert!(!turn.password.is_empty());
}
