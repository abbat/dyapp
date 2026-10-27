use super::message;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The circuit relay v2 slots of role relay, see docs/decisions/0031. libp2p's reservation and
/// circuit rate limits per peer and IP apply on top.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Relay {
    /// Reservations held at once.
    pub max_reservations: usize,
    /// Reservations of one peer.
    pub max_reservations_per_peer: usize,
    /// How long a reservation lasts before the peer renews it.
    pub reservation_minutes: u32,
    /// Circuits open at once.
    pub max_circuits: usize,
    /// Circuits to or from one peer.
    pub max_circuits_per_peer: usize,
    /// How long one circuit stays open.
    pub circuit_seconds: u32,
    /// Bytes one circuit carries in each direction before it closes.
    pub circuit_kb: u64,
}

impl Default for Relay {
    fn default() -> Self {
        Self {
            max_reservations: 128,
            max_reservations_per_peer: 4,
            reservation_minutes: 60,
            max_circuits: 16,
            max_circuits_per_peer: 4,
            circuit_seconds: 120,
            circuit_kb: 128,
        }
    }
}

impl Relay {
    pub(super) fn validate(&self) -> anyhow::Result<()> {
        if self.max_reservations < 1
            || self.max_reservations_per_peer < 1
            || self.reservation_minutes < 1
            || self.max_circuits < 1
            || self.max_circuits_per_peer < 1
            || self.circuit_seconds < 1
            || self.circuit_kb < 1
        {
            anyhow::bail!(message(
                "every relay setting must be at least 1",
                "каждый параметр relay должен быть не меньше 1"
            ));
        }
        Ok(())
    }

    /// The libp2p hop config with these limits and libp2p's rate limits.
    pub(super) fn libp2p(&self) -> libp2p::relay::Config {
        libp2p::relay::Config {
            max_reservations: self.max_reservations,
            max_reservations_per_peer: self.max_reservations_per_peer,
            reservation_duration: Duration::from_secs(u64::from(self.reservation_minutes) * 60),
            max_circuits: self.max_circuits,
            max_circuits_per_peer: self.max_circuits_per_peer,
            max_circuit_duration: Duration::from_secs(self.circuit_seconds.into()),
            max_circuit_bytes: self.circuit_kb.saturating_mul(1 << 10),
            ..libp2p::relay::Config::default()
        }
    }
}
