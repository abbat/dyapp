//! Admission and drain for one active desktop identity.
//!
//! A command owns a lease until its async or blocking work is finished. A transition closes
//! admission first, then waits without holding the state mutex. The Tauri command wiring follows
//! in the next checkpoint; keeping this primitive independent makes its ordering testable.

use std::sync::{Arc, Condvar, Mutex};

/// A failed attempt to enter or complete an identity transition.
#[derive(Debug, PartialEq, Eq)]
pub enum GateError {
    /// No identity is selected.
    NoActive,
    /// A switch or removal has closed admission.
    Transitioning,
    /// A lease is still running, so the transition cannot commit.
    NotDrained,
    /// The gate state was poisoned by a panicking holder.
    Poisoned,
}

struct State<T> {
    active: Option<T>,
    generation: u64,
    leases: usize,
    transitioning: bool,
}

/// The current identity and the count of commands allowed to use it.
pub struct IdentityGate<T> {
    state: Mutex<State<T>>,
    drained: Condvar,
}

impl<T: Clone> IdentityGate<T> {
    /// Creates a gate with an optional selected identity.
    pub fn new(active: Option<T>) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                active,
                generation: 0,
                leases: 0,
                transitioning: false,
            }),
            drained: Condvar::new(),
        })
    }

    /// Admits work on the selected identity and returns its stable value and generation.
    pub fn lease(self: &Arc<Self>) -> Result<Lease<T>, GateError> {
        let mut state = self.state.lock().map_err(|_| GateError::Poisoned)?;
        if state.transitioning {
            return Err(GateError::Transitioning);
        }
        let active = state.active.clone().ok_or(GateError::NoActive)?;
        state.leases += 1;
        Ok(Lease {
            gate: Arc::clone(self),
            active,
            generation: state.generation,
        })
    }

    /// Closes admission immediately; the caller then cancels work and waits to drain.
    pub fn start_transition(self: &Arc<Self>) -> Result<Transition<T>, GateError> {
        let mut state = self.state.lock().map_err(|_| GateError::Poisoned)?;
        if state.transitioning {
            return Err(GateError::Transitioning);
        }
        state.transitioning = true;
        Ok(Transition {
            gate: Arc::clone(self),
            committed: false,
        })
    }
}

/// One command's permission to use the selected identity until this value is dropped.
pub struct Lease<T> {
    gate: Arc<IdentityGate<T>>,
    active: T,
    generation: u64,
}

impl<T> Lease<T> {
    /// The identity value selected when this command was admitted.
    pub fn active(&self) -> &T {
        &self.active
    }

    /// The selected identity's generation when this command was admitted.
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

impl<T> Drop for Lease<T> {
    fn drop(&mut self) {
        // A panicking worker must still release its lease so the next identity can open.
        let mut state = self.gate.state.lock().unwrap_or_else(|error| error.into_inner());
        state.leases -= 1;
        if state.leases == 0 {
            self.gate.drained.notify_all();
        }
    }
}

/// Exclusive switch or removal, with rollback to the previous value on drop.
pub struct Transition<T> {
    gate: Arc<IdentityGate<T>>,
    committed: bool,
}

impl<T> Transition<T> {
    /// Waits for admitted work to finish; call this on a blocking thread.
    pub fn wait(&self) -> Result<(), GateError> {
        let mut state = self.gate.state.lock().map_err(|_| GateError::Poisoned)?;
        while state.leases != 0 {
            state = self.gate.drained.wait(state).map_err(|_| GateError::Poisoned)?;
        }
        Ok(())
    }

    /// Replaces the selected identity after every lease has finished.
    pub fn commit(mut self, active: Option<T>) -> Result<(), GateError> {
        let mut state = self.gate.state.lock().map_err(|_| GateError::Poisoned)?;
        if state.leases != 0 {
            return Err(GateError::NotDrained);
        }
        state.active = active;
        state.generation += 1;
        state.transitioning = false;
        self.committed = true;
        Ok(())
    }
}

impl<T> Drop for Transition<T> {
    fn drop(&mut self) {
        if !self.committed {
            // An open or remove failure leaves the previous explicit selection available.
            let mut state = self.gate.state.lock().unwrap_or_else(|error| error.into_inner());
            state.transitioning = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn transition_closes_admission_and_drains_a_paused_lease() {
        let gate = IdentityGate::new(Some("alice"));
        let lease = gate.lease().unwrap();
        let transition = gate.start_transition().unwrap();
        assert_eq!(gate.lease().err(), Some(GateError::Transitioning));
        assert_eq!(gate.start_transition().err(), Some(GateError::Transitioning));
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            transition.wait().unwrap();
            transition.commit(Some("bob")).unwrap();
            done_tx.send(()).unwrap();
        });
        drop(lease);
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let next = gate.lease().unwrap();
        assert_eq!(*next.active(), "bob");
        assert_eq!(next.generation(), 1);
    }

    #[test]
    fn unsuccessful_transition_restores_admission_to_the_old_identity() {
        let gate = IdentityGate::new(Some("alice"));
        let transition = gate.start_transition().unwrap();
        assert_eq!(gate.lease().err(), Some(GateError::Transitioning));
        drop(transition);
        assert_eq!(*gate.lease().unwrap().active(), "alice");
        let transition = gate.start_transition().unwrap();
        transition.wait().unwrap();
        transition.commit(None).unwrap();
        assert_eq!(gate.lease().err(), Some(GateError::NoActive));
    }

    #[test]
    fn commit_refuses_to_bypass_a_live_lease() {
        let gate = IdentityGate::new(Some("alice"));
        let lease = gate.lease().unwrap();
        let transition = gate.start_transition().unwrap();
        assert_eq!(transition.commit(Some("bob")), Err(GateError::NotDrained));
        drop(lease);
        assert_eq!(*gate.lease().unwrap().active(), "alice");
    }
}
