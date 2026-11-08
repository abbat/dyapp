use super::*;

#[test]
fn local_status_exposes_only_typed_retry_metadata_and_generation() {
    let token = [3; 16];
    let waiting = Status::download(
        &PackDownloadState {
            has_answer: false,
            failure: None,
            attempts: 0,
            retry_at_ms: 0,
        },
        token,
    );
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
        let status = Status::download(
            &PackDownloadState {
                has_answer: true,
                failure,
                attempts: 1,
                retry_at_ms: 1234,
            },
            token,
        );
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

#[test]
fn installed_pack_membership_does_not_accept_a_claimed_unrelated_sticker() {
    let pack =
        dyapp_client::sticker_pack::import_tar(&super::super::native_tests::archive_count(1))
            .unwrap();
    assert!(contains(&pack, &pack.stickers()[0].sha256));
    assert!(!contains(&pack, &[99; 32]));
}
