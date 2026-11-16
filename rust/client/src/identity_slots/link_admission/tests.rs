use super::*;

#[test]
fn only_one_poll_is_admitted_and_normal_completion_allows_the_next() {
    let admission = LinkAdmission::default();
    assert_eq!(admission.phase(), LinkPhase::Waiting);
    let poll = admission.receive().unwrap();
    assert_eq!(admission.phase(), LinkPhase::Receiving);
    assert!(matches!(admission.receive(), Err(LinkPhase::Receiving)));
    assert_eq!(poll.resume(), LinkPhase::Waiting);
    assert_eq!(admission.receive().unwrap().resume(), LinkPhase::Waiting);
}

#[test]
fn cancellation_is_terminal_before_or_during_receiving() {
    for receiving in [false, true] {
        let admission = LinkAdmission::default();
        let poll = receiving.then(|| admission.receive().unwrap());
        assert_eq!(admission.cancel(), LinkPhase::Cancelled);
        assert_eq!(admission.cancel(), LinkPhase::Cancelled);
        if let Some(poll) = poll {
            assert_eq!(poll.resume(), LinkPhase::Cancelled);
        }
        assert!(matches!(admission.receive(), Err(LinkPhase::Cancelled)));
    }
}

#[test]
fn cancelling_before_write_admission_prevents_the_writer_guard() {
    let admission = LinkAdmission::default();
    let view = ProfileView::default();
    let generation = view.current().unwrap().generation();
    let poll = admission.receive().unwrap();
    assert_eq!(admission.cancel(), LinkPhase::Cancelled);
    assert!(poll.install(&view, &generation).is_err());
    assert_eq!(admission.phase(), LinkPhase::Cancelled);
}

#[test]
fn an_aba_profile_transition_invalidates_write_admission() {
    let admission = LinkAdmission::default();
    let view = ProfileView::default();
    let generation = view.current().unwrap().generation();
    let poll = admission.receive().unwrap();
    drop(view.transition().unwrap());
    drop(view.transition().unwrap());
    assert!(poll.install(&view, &generation).is_err());
    assert_eq!(admission.phase(), LinkPhase::Invalidated);
    assert_eq!(admission.cancel(), LinkPhase::Invalidated);
    assert!(matches!(admission.receive(), Err(LinkPhase::Invalidated)));
}

#[test]
fn foreign_and_poisoned_views_do_not_admit_a_write() {
    let view = ProfileView::default();
    let generation = view.current().unwrap().generation();
    let foreign = ProfileView::default();
    let admission = LinkAdmission::default();
    assert!(admission
        .receive()
        .unwrap()
        .install(&foreign, &generation)
        .is_err());
    assert_eq!(admission.phase(), LinkPhase::Invalidated);
    let unwound = std::panic::catch_unwind(|| {
        let _guard = view.transition().unwrap();
        panic!("controlled view unwind");
    });
    assert!(unwound.is_err());
    let admission = LinkAdmission::default();
    assert!(admission
        .receive()
        .unwrap()
        .install(&view, &generation)
        .is_err());
    assert_eq!(admission.phase(), LinkPhase::Invalidated);
}

#[test]
fn cancellation_after_admission_never_claims_rollback_or_changes_the_receipt() {
    for succeeded in [false, true] {
        let admission = LinkAdmission::default();
        let view = ProfileView::default();
        let generation = view.current().unwrap().generation();
        let write = admission
            .receive()
            .unwrap()
            .install(&view, &generation)
            .unwrap();
        assert_eq!(admission.cancel(), LinkPhase::Installing);
        assert!(matches!(admission.receive(), Err(LinkPhase::Installing)));
        write.finish(succeeded);
        let expected = if succeeded {
            LinkPhase::Completed
        } else {
            LinkPhase::Failed
        };
        assert_eq!(admission.phase(), expected);
        assert_eq!(admission.cancel(), expected);
        assert!(matches!(admission.receive(), Err(phase) if phase == expected));
    }
}

#[test]
fn abandoned_or_unwinding_work_is_interrupted_and_cannot_retry() {
    let receiving = LinkAdmission::default();
    drop(receiving.receive().unwrap());
    assert_eq!(receiving.phase(), LinkPhase::Interrupted);
    assert!(matches!(receiving.receive(), Err(LinkPhase::Interrupted)));
    let installing = LinkAdmission::default();
    let view = ProfileView::default();
    let generation = view.current().unwrap().generation();
    let unwound = std::panic::catch_unwind(|| {
        let _write = installing
            .receive()
            .unwrap()
            .install(&view, &generation)
            .unwrap();
        panic!("controlled writer unwind");
    });
    assert!(unwound.is_err());
    assert_eq!(installing.phase(), LinkPhase::Interrupted);
    assert_eq!(installing.cancel(), LinkPhase::Interrupted);
    assert!(matches!(installing.receive(), Err(LinkPhase::Interrupted)));
}

#[test]
fn an_admitted_installation_invalidates_other_invitations_even_if_it_fails() {
    let view = ProfileView::default();
    let generation = view.current().unwrap().generation();
    let first = LinkAdmission::default();
    let other = LinkAdmission::default();
    let other_poll = other.receive().unwrap();
    first
        .receive()
        .unwrap()
        .install(&view, &generation)
        .unwrap()
        .finish(false);
    assert!(other_poll.install(&view, &generation).is_err());
    assert_eq!(other.phase(), LinkPhase::Invalidated);
    assert_eq!(first.phase(), LinkPhase::Failed);
}

#[test]
fn dropping_cancelled_work_does_not_resurrect_or_relabel_it() {
    let admission = LinkAdmission::default();
    let poll = admission.receive().unwrap();
    assert_eq!(admission.cancel(), LinkPhase::Cancelled);
    drop(poll);
    assert_eq!(admission.phase(), LinkPhase::Cancelled);
}
