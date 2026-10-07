use dyapp_bootstrap::{
    api::AppState, rate_limit::PeerRateLimiter, BootstrapConfig, BootstrapServer, BootstrapStore,
};
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use prost::Message;
use std::sync::Arc;

/// `test-peer sign-profile` prints `{"peer_id", "record"}` with a freshly signed profile as hex
/// protobuf, for scripts that have no Ed25519 or protobuf library. Without arguments it serves.
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

    let config = BootstrapConfig {
        storage_path: "/tmp/ai/bootstrap".to_string(),
        ..BootstrapConfig::default()
    };
    std::fs::create_dir_all("/tmp/ai")?;
    let state = AppState {
        store: Arc::new(BootstrapStore::new(&config.storage_path)?),
        rate_limiter: Arc::new(PeerRateLimiter::new(100)),
        config,
    };
    let listener = tokio::net::TcpListener::bind("0.0.0.0:7070").await?;
    axum::serve(listener, BootstrapServer::router(state)).await?;
    Ok(())
}
