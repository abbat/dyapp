use super::*;

#[test]
fn node_refusals_keep_distinct_native_codes() {
    let code = |status| send_error(session::Error::Client(Error::Status(status))).to_string();
    assert_eq!(code(Status::Full), "full");
    assert_eq!(code(Status::TooLarge), "tooLarge");
    assert_eq!(code(Status::NotFound), "peerUnavailable");
    assert_ne!(code(Status::Denied), "full");
}
