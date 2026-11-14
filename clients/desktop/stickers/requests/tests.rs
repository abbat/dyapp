use super::*;
use dyapp_client::store::sticker_packs::exchange::download::{
    PackDownloadFailure, PackDownloadState,
};

#[test]
fn new_saved_and_missing_sticker_keep_the_existing_json_contract() {
    for (state, name) in [
        (pack_requests::State::New, "new"),
        (pack_requests::State::Saved, "saved"),
    ] {
        let value = Status::from(pack_requests::Status {
            state,
            generation: None,
            failure: None,
            retry_at_ms: None,
        });
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            serde_json::json!({
                "state":name, "generation":null, "failure":null, "retryAtMs":null,
            })
        );
    }
    let token = [3_u8; 16];
    let value = Status::from(pack_requests::Status {
        state: pack_requests::State::Failed,
        generation: Some(token),
        failure: Some(pack_requests::Failure::StickerMissing),
        retry_at_ms: None,
    });
    let json = serde_json::to_value(value).unwrap();
    assert_eq!(json["failure"], "stickerMissing");
    assert_eq!(json["generation"], serde_json::json!(token));
}

#[test]
fn local_status_exposes_only_typed_retry_metadata_and_generation() {
    let token = [3; 16];
    let waiting: Status = pack_requests::Status::from_download(
        &PackDownloadState {
            has_answer: false,
            failure: None,
            attempts: 0,
            retry_at_ms: 0,
        },
        token,
    )
    .into();
    let json = serde_json::to_value(waiting).unwrap();
    assert_eq!(json["state"], "waiting");
    assert!(json["retryAtMs"].is_null() && json["failure"].is_null());
    for (failure, name) in [
        (None, "downloading"),
        (Some(PackDownloadFailure::Fetch), "failed"),
        (Some(PackDownloadFailure::Invalid), "failed"),
        (Some(PackDownloadFailure::Quota), "failed"),
        (Some(PackDownloadFailure::Storage), "failed"),
    ] {
        let status: Status = pack_requests::Status::from_download(
            &PackDownloadState {
                has_answer: true,
                failure,
                attempts: 1,
                retry_at_ms: 1234,
            },
            token,
        )
        .into();
        let json = serde_json::to_value(status).unwrap();
        assert_eq!(json["state"], name);
        assert_eq!(json["retryAtMs"], 1234);
        assert_eq!(json["generation"], serde_json::json!(token));
        let mut fields: Vec<_> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(fields, ["failure", "generation", "retryAtMs", "state"]);
    }
}
