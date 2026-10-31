//! The desktop's active identity and transition admission.
use crate::identity_gate::{GateError, IdentityGate, Lease, Transition};
use crate::slots::{self, Opened};
use dyapp_identity::Identity;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) type ActiveIdentity = (Arc<Identity>, PathBuf);

/// The one source of truth for the selected identity. Command-held leases are wired separately.
pub(super) struct IdentityState(Arc<IdentityGate<ActiveIdentity>>);

impl IdentityState {
    pub(super) fn new() -> Self {
        Self(IdentityGate::new(None))
    }

    /// Keep this value alive through the command and all of its awaited work.
    pub(super) fn lease(&self) -> Result<Lease<ActiveIdentity>, String> {
        self.0.lease().map_err(gate_error)
    }

    /// Closes admission before a command awaits the old slot's work.
    pub(super) fn start_transition(&self) -> Result<Transition<ActiveIdentity>, String> {
        self.0.start_transition().map_err(gate_error)
    }

    /// Wait without blocking Tauri's async executor. Dropping a failed transition reopens admission.
    pub(super) async fn drain(
        transition: Transition<ActiveIdentity>,
    ) -> Result<Transition<ActiveIdentity>, String> {
        tauri::async_runtime::spawn_blocking(move || {
            transition.wait().map_err(gate_error)?;
            Ok::<_, String>(transition)
        })
        .await
        .map_err(|error| error.to_string())?
    }

    /// A failed slot mutation may have changed its key or active pointer. Reopen only the
    /// persisted selection; never silently admit a different remaining identity.
    pub(super) async fn reconcile(
        root: PathBuf,
        transition: Transition<ActiveIdentity>,
    ) -> Result<(), String> {
        let previous_dir = transition
            .previous()
            .map_err(gate_error)?
            .map(|(_, dir)| dir);
        let opened = tauri::async_runtime::spawn_blocking(move || {
            let active_id = slots::active(&root);
            let selected = slots::list(&root)?.into_iter().find(|slot| {
                active_id.as_ref().map_or_else(
                    || previous_dir.as_ref() == Some(&slot.dir),
                    |id| id == &slot.id,
                )
            });
            match selected {
                Some(slot) => slots::open_slot(slot, &super::load),
                None => Ok(Opened::Picker),
            }
        })
        .await;
        let opened = match opened {
            Ok(Ok(opened)) => opened,
            _ => Opened::Picker,
        };
        respond(opened, transition).map(|_| ())
    }
}

fn gate_error(error: GateError) -> String {
    match error {
        GateError::NoActive => "No identity is open".into(),
        GateError::Transitioning => "Identity is changing".into(),
        GateError::NotDrained => "Identity transition has running work".into(),
        GateError::Poisoned => "Identity state is unavailable".into(),
    }
}

pub(super) fn ready_result(
    mode: &str,
    peer_id: String,
    dir: &Path,
) -> HashMap<&'static str, String> {
    let mut result = HashMap::from([
        ("status", "ready".into()),
        ("mode", mode.to_owned()),
        ("peerId", peer_id),
    ]);
    if mode == "file" {
        result.insert("keyPath", dir.join("identity.key").display().to_string());
    }
    result
}

/// The page's view of an opened slot and its unlocked identity, if any.
pub(super) fn describe(opened: Opened) -> (HashMap<&'static str, String>, Option<ActiveIdentity>) {
    let mut result = HashMap::new();
    match opened {
        Opened::Ready(slot, identity) => {
            result = ready_result(slot.mode.unwrap_or_default(), identity.peer_id(), &slot.dir);
            result.insert("slot", slot.id);
            return (result, Some((Arc::new(identity), slot.dir)));
        }
        Opened::Setup => {
            result.insert("status", "setup".into());
        }
        Opened::Picker => {
            result.insert("status", "picker".into());
        }
        Opened::Recovery(slot, reason, peer_id) => {
            result.insert("status", "recovery".into());
            result.insert("reason", reason.into());
            result.insert("slot", slot.id);
            result.extend(slot.mode.map(|mode| ("mode", mode.into())));
            result.extend(peer_id.map(|peer_id| ("peerId", peer_id)));
        }
    }
    (result, None)
}

/// Commits an opened identity only after admitted work has drained.
pub(super) fn respond(
    opened: Opened,
    transition: Transition<ActiveIdentity>,
) -> Result<HashMap<&'static str, String>, String> {
    let (result, identity) = describe(opened);
    transition.commit(identity).map_err(gate_error)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[tokio::test]
    async fn cancelled_caller_cannot_release_a_running_blocking_workers_lease() {
        let state = IdentityState::new();
        state
            .start_transition()
            .unwrap()
            .commit(Some((
                Arc::new(Identity::generate()),
                PathBuf::from("first"),
            )))
            .unwrap();
        let lease = state.lease().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let caller = tokio::spawn(async move {
            tauri::async_runtime::spawn_blocking(move || {
                let _lease = lease;
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            })
            .await
            .unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        caller.abort();
        let transition = state.start_transition().unwrap();
        assert_eq!(transition.commit(None), Err(GateError::NotDrained));
        release_tx.send(()).unwrap();
        let transition = state.start_transition().unwrap();
        IdentityState::drain(transition)
            .await
            .unwrap()
            .commit(None)
            .unwrap();
    }

    #[tokio::test]
    async fn selection_waits_for_a_paused_lease_and_failed_selection_rolls_back() {
        let state = IdentityState::new();
        let first = Identity::generate();
        let transition = state.start_transition().unwrap();
        transition
            .commit(Some((Arc::new(first), PathBuf::from("first"))))
            .unwrap();
        let lease = state.0.lease().unwrap();
        let transition = state.start_transition().unwrap();
        assert!(matches!(state.0.lease(), Err(GateError::Transitioning)));
        let draining = tokio::spawn(IdentityState::drain(transition));
        assert!(!draining.is_finished());
        drop(lease);
        let transition = draining.await.unwrap().unwrap();
        let second = Identity::generate();
        transition
            .commit(Some((Arc::new(second), PathBuf::from("second"))))
            .unwrap();
        assert_eq!(state.lease().unwrap().active().1, PathBuf::from("second"));
        let transition = state.start_transition().unwrap();
        drop(transition);
        assert_eq!(state.lease().unwrap().active().1, PathBuf::from("second"));
    }
}
