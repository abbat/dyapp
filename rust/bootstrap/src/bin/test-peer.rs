use dyapp_bootstrap::{
    api::AppState, rate_limit::PeerRateLimiter, BootstrapServer, BootstrapStore, NodeConfig,
};
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use prost::Message;
use std::sync::Arc;

/// `test-peer sign-profile` prints `{"peer_id", "record"}` with a freshly signed profile as hex
/// protobuf, for scripts that have no Ed25519 or protobuf library. Without arguments it serves on
/// `TEST_PEER_ADDR` (default `0.0.0.0:7070`) with storage in `TEST_PEER_STORAGE`.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("sign-profile") {
        let identity = Identity::generate();
        let record = Profile {
            version: 1,
            age: 26,
            ..Profile::default()
        }
        .sign(&identity);
        let hex: String = record
            .encode_to_vec()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        println!(
            "{}",
            serde_json::json!({ "peer_id": identity.peer_id(), "record": hex })
        );
        return Ok(());
    }

    let storage =
        std::env::var("TEST_PEER_STORAGE").unwrap_or_else(|_| "/tmp/ai/bootstrap".to_string());
    let config = NodeConfig::default();
    let state = AppState {
        store: Arc::new(BootstrapStore::new(&storage)?),
        rate_limiter: Arc::new(PeerRateLimiter::new(config.limits.requests_per_second)),
        config,
    };
    let address = std::env::var("TEST_PEER_ADDR").unwrap_or_else(|_| "0.0.0.0:7070".to_string());
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, BootstrapServer::router(state)).await?;
    Ok(())
}
