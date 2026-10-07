use crate::{
    error::{BootstrapError, Result},
    rate_limit::PeerRateLimiter,
    BootstrapConfig, BootstrapStore, MessageBlob,
};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use dyapp_identity::SignedRecord;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<BootstrapStore>,
    pub rate_limiter: Arc<PeerRateLimiter>,
    pub config: BootstrapConfig,
}

#[derive(Serialize, Deserialize)]
pub struct StoreMessageRequest {
    pub sender_id: String,
    pub recipient_id: String,
    pub encrypted_payload: Vec<u8>,
}

/// Response body of `GET /profiles`.
#[derive(Clone, PartialEq, prost::Message)]
pub struct ProfileList {
    #[prost(message, repeated, tag = "1")]
    pub profiles: Vec<SignedRecord>,
}

const PROTOBUF: &str = "application/x-protobuf";

#[derive(Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: i64,
}

#[derive(Serialize, Deserialize)]
pub struct ListProfilesQuery {
    pub skip: Option<u32>,
    pub limit: Option<u32>,
}

pub struct BootstrapServer;

impl BootstrapServer {
    pub fn router(state: AppState) -> Router {
        Router::new()
            .route("/health", get(health_check))
            .route("/messages", post(store_message))
            .route(
                "/messages/{message_id}",
                get(get_message).delete(delete_message),
            )
            .route("/messages/peer/{peer_id}", get(get_peer_messages))
            .route("/profiles", post(store_profile).get(list_profiles))
            .route("/profiles/{peer_id}", get(get_profile))
            .with_state(state)
            .layer(CorsLayer::permissive())
    }
}

async fn health_check(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    match state.store.health_check() {
        Ok(_) => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "healthy".to_string(),
                timestamp,
            }),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "unhealthy".to_string(),
                timestamp,
            }),
        ),
    }
}

async fn store_message(
    State(state): State<AppState>,
    Json(req): Json<StoreMessageRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    if !state.rate_limiter.check_limit(&req.sender_id) {
        return Err(BootstrapError::RateLimitExceeded);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let msg = MessageBlob {
        id: uuid::Uuid::new_v4().to_string(),
        sender_id: req.sender_id,
        recipient_id: req.recipient_id,
        encrypted_payload: req.encrypted_payload,
        timestamp: now,
        ttl_expires_at: now + (state.config.message_ttl_hours as i64 * 3600),
    };

    let msg_id = msg.id.clone();
    state.store.store_message(msg)?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "message_id": msg_id })),
    ))
}

async fn get_message(
    State(state): State<AppState>,
    Path(message_id): Path<String>,
) -> Result<Json<MessageBlob>> {
    state
        .store
        .get_message(&message_id)?
        .ok_or(BootstrapError::MessageNotFound)
        .map(Json)
}

async fn delete_message(
    State(state): State<AppState>,
    Path(message_id): Path<String>,
) -> Result<StatusCode> {
    state.store.delete_message(&message_id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_peer_messages(
    State(state): State<AppState>,
    Path(peer_id): Path<String>,
) -> Result<Json<Vec<MessageBlob>>> {
    let messages = state.store.get_messages_for_peer(&peer_id)?;
    Ok(Json(messages))
}

fn protobuf(body: Vec<u8>) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, PROTOBUF)], body)
}

/// Body: a protobuf `SignedRecord` holding a profile. Deletion is a signed tombstone.
async fn store_profile(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    let record =
        SignedRecord::decode(body).map_err(|e| BootstrapError::InvalidRequest(e.to_string()))?;
    let public_key: [u8; 32] = record
        .public_key
        .as_slice()
        .try_into()
        .map_err(|_| BootstrapError::Profile(dyapp_identity::Error::InvalidKey.into()))?;
    if !state
        .rate_limiter
        .check_limit(&dyapp_identity::peer_id(&public_key))
    {
        return Err(BootstrapError::RateLimitExceeded);
    }

    let verified = state.store.put_profile(&record)?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "peer_id": verified.peer_id,
            "version": verified.profile.version,
        })),
    ))
}

/// Returns the stored `SignedRecord`, tombstones included, so peers learn of deletions.
async fn get_profile(
    State(state): State<AppState>,
    Path(peer_id): Path<String>,
) -> Result<impl IntoResponse> {
    let record = state
        .store
        .get_profile(&peer_id)?
        .ok_or(BootstrapError::ProfileNotFound)?;
    Ok(protobuf(record.encode_to_vec()))
}

async fn list_profiles(
    State(state): State<AppState>,
    Query(params): Query<ListProfilesQuery>,
) -> Result<impl IntoResponse> {
    let skip = params.skip.unwrap_or(0);
    let limit = params.limit.unwrap_or(100);

    let profiles = state.store.list_profiles(skip, limit)?;
    Ok(protobuf(ProfileList { profiles }.encode_to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_app_state_creation() {
        let store = Arc::new(BootstrapStore::new("/tmp/test-api").unwrap());
        let rate_limiter = Arc::new(PeerRateLimiter::new(10));
        let config = BootstrapConfig::default();

        let _state = AppState {
            store,
            rate_limiter,
            config,
        };
    }
}
