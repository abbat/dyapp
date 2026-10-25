//! Message records, shared logical time and MLS groups.

/// Hybrid logical timestamps and deterministic ordering.
pub mod hlc;
/// Serializable message fields and caller-assigned status.
pub mod message;
/// End-to-end encrypted groups over MLS.
pub mod mls;

pub use hlc::{HybridClock, Timestamp};
pub use message::{Message, MessageStatus};
