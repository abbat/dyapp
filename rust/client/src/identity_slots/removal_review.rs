//! Locked local removal guidance; this snapshot is never a deletion authorization.
use super::{open_slot, removal_allowed, KeyReader, Registry, Resolution};

/// Guidance for the existing confirmation dialog, derived from current local inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemovalReview {
    /// Whether this resolution may offer the confirmed-removal flow.
    pub allowed: bool,
    /// Another ready local candidate opened with the same Peer ID; false retains the last-copy warning.
    pub has_other_copy: bool,
}

impl Registry {
    /// Recheck the displayed candidate and inspect local duplicate keys without selecting or mutating it.
    ///
    /// The shared registry lock keeps this inventory consistent with cooperating mutations. Actual
    /// removal must still independently confirm its target, key and owned files at execution time.
    ///
    /// # Errors
    /// Returns lock/inventory failures, a missing target or a Peer ID differing from the displayed one.
    pub fn removal_review(
        &self,
        id: &str,
        expected_peer: Option<&str>,
        keys: &dyn KeyReader,
    ) -> Result<RemovalReview, String> {
        let _guard = self.shared_lock()?;
        let slots = super::list(&self.root, self.layout).map_err(|error| error.to_string())?;
        let target = slots
            .iter()
            .find(|slot| slot.id == id)
            .ok_or("Identity slot is missing")?;
        let (peer_id, reason) = match open_slot(target.clone(), keys) {
            Resolution::Ready(_, identity) => (Some(identity.peer_id()), None),
            Resolution::Recovery {
                peer_id, reason, ..
            } => (peer_id, Some(reason)),
            Resolution::Setup | Resolution::Picker(_) => {
                return Err("Identity slot did not resolve".into())
            }
        };
        if peer_id.as_deref() != expected_peer {
            return Err("The key does not match the displayed Peer ID".into());
        }
        let allowed = removal_allowed(Some(id), reason, true);
        let has_other_copy = peer_id.is_some_and(|peer_id| {
            slots.into_iter().filter(|slot| slot.id != id).any(|slot| {
                matches!(open_slot(slot, keys), Resolution::Ready(_, identity) if identity.peer_id() == peer_id)
            })
        });
        Ok(RemovalReview {
            allowed,
            has_other_copy,
        })
    }
}
