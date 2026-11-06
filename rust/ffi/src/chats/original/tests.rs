use super::*;

#[test]
fn missing_corrupted_and_transport_failures_remain_distinct() {
    assert_eq!(
        fetch_error(Error::Status(Status::NotFound)).to_string(),
        "notFound"
    );
    assert_eq!(
        fetch_error(Error::Attachment("bad tag")).to_string(),
        "corrupted"
    );
    let transport = fetch_error(Error::Timeout).to_string();
    assert_ne!(transport, "notFound");
    assert_ne!(transport, "corrupted");
}

#[test]
fn invalid_original_bytes_never_reach_native_decoders() {
    assert!(!acceptable(b"not an image", true));
}
