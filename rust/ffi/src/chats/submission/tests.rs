use super::*;
use dyapp_p2p_net::proto::Status;

#[test]
fn accepted_keeps_the_operation_id_and_legacy_success() {
    let native = ChatSubmission::from(Submission::Accepted { id: [7; 16] });
    assert_eq!(native, ChatSubmission::Accepted { id: vec![7; 16] });
    assert_eq!(native.into_accepted().unwrap(), vec![7; 16]);
}

#[test]
fn queued_keeps_commitment_while_legacy_calls_still_fail() {
    let error = Error::Client(dyapp_client::Error::Status(Status::NotFound));
    let detail = error.to_string();
    let native = ChatSubmission::from(Submission::Queued { id: [8; 16], error });
    assert_eq!(
        native,
        ChatSubmission::Queued {
            id: vec![8; 16],
            detail: detail.clone()
        }
    );
    assert_eq!(native.into_accepted().unwrap_err().to_string(), detail);
}

#[test]
fn operation_specific_error_mapping_never_erases_commitment() {
    let native = ChatSubmission::from_core(
        Submission::Queued {
            id: [9; 16],
            error: Error::Client(dyapp_client::Error::Status(Status::Full)),
        },
        |_| failed("full"),
    );
    assert_eq!(
        native,
        ChatSubmission::Queued {
            id: vec![9; 16],
            detail: "full".into()
        }
    );
    assert_eq!(native.into_accepted().unwrap_err().to_string(), "full");
}
