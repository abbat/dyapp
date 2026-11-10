use super::*;
use dyapp_client::interactions::{Interaction, Kind};
use dyapp_client::session::Error;
use dyapp_p2p_net::proto::Status;

fn likes() -> ChatLikes {
    ChatLikes {
        ours: true,
        matched: false,
        passed: false,
    }
}

fn committed(delivery: Result<(), Error>) -> InteractionSubmission {
    InteractionSubmission {
        event: Interaction {
            kind: Kind::Like,
            event_id: [7; 16],
            hlc: 1_000,
        },
        delivery,
    }
}

#[test]
fn accepted_retains_identity_and_legacy_projection() {
    let result = ChatLikeAttempt::new(Some(committed(Ok(()))), Ok(likes()));
    assert_eq!(
        result.submission,
        Some(ChatSubmission::Accepted { id: vec![7; 16] })
    );
    assert_eq!(result.state, ChatLikeState::Ready { likes: likes() });
    assert_eq!(result.into_accepted().unwrap(), likes());
}

#[test]
fn queued_identity_survives_a_projection_failure_and_delivery_error_takes_precedence() {
    let cause = Error::Client(dyapp_client::Error::Status(Status::NotFound));
    let delivery_error = cause.to_string();
    let result = ChatLikeAttempt::new(Some(committed(Err(cause))), Err(failed("read failure")));
    assert_eq!(
        result.submission,
        Some(ChatSubmission::Queued {
            id: vec![7; 16],
            detail: delivery_error.clone()
        })
    );
    assert_eq!(
        result.state,
        ChatLikeState::Unavailable {
            detail: "read failure".into()
        }
    );
    assert_eq!(
        result.into_accepted().unwrap_err().to_string(),
        delivery_error
    );
}

#[test]
fn accepted_commitment_is_not_lost_when_the_later_read_fails() {
    let result = ChatLikeAttempt::new(Some(committed(Ok(()))), Err(failed("read failure")));
    assert_eq!(
        result.submission,
        Some(ChatSubmission::Accepted { id: vec![7; 16] })
    );
    assert_eq!(
        result.state,
        ChatLikeState::Unavailable {
            detail: "read failure".into()
        }
    );
    assert_eq!(
        result.into_accepted().unwrap_err().to_string(),
        "read failure"
    );
}

#[test]
fn matching_state_and_read_errors_never_fabricate_an_event() {
    let result = ChatLikeAttempt::new(None, Ok(likes()));
    assert_eq!(result.submission, None);
    assert_eq!(result.into_accepted().unwrap(), likes());
    let result = ChatLikeAttempt::new(None, Err(failed("read failure")));
    assert_eq!(result.submission, None);
    assert_eq!(
        result.into_accepted().unwrap_err().to_string(),
        "read failure"
    );
}
