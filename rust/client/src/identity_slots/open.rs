use super::{active, keys, list, Active, KeyReader, Layout, Slot, State, Storage, LEGACY};
use dyapp_identity::Identity;
use std::path::Path;

/// Stable reason requiring explicit user recovery instead of key replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryReason {
    /// No confirmed key exists at the selected location.
    Missing,
    /// The key store or key bytes cannot currently be opened.
    Unavailable,
    /// Creation has not reached ready.
    Incomplete,
    /// Confirmed deletion has not finished.
    Deleting,
    /// Lifecycle or storage metadata is damaged.
    Damaged,
    /// A saved active pointer is malformed and there is no candidate fallback.
    InvalidPointer,
}

/// Result of resolving existing identities; this API never creates keys or writes files.
pub enum Resolution {
    /// No candidates, pointer or orphan legacy credentials exist.
    Setup,
    /// Explicit selection is required.
    Picker(Vec<Slot>),
    /// An existing key opened successfully.
    Ready(Slot, Box<Identity>),
    /// User action is required; an ID may refer to a now-missing slot.
    Recovery {
        /// Slot or pointer target, absent when the pointer cannot be interpreted.
        id: Option<String>,
        /// Stable recovery classification.
        reason: RecoveryReason,
        /// Derived Peer ID when an incomplete/deleting slot's key still opens.
        peer_id: Option<String>,
    },
}

fn recovery(id: Option<String>, reason: RecoveryReason) -> Resolution {
    Resolution::Recovery {
        id,
        reason,
        peer_id: None,
    }
}

/// Open one inventoried candidate without changing its lifecycle or generating keys.
#[must_use]
pub fn open_slot(slot: Slot, keys: &dyn KeyReader) -> Resolution {
    if slot.state == State::Broken {
        return recovery(Some(slot.id), RecoveryReason::Damaged);
    }
    let Ok(identity) = keys::load(&slot, keys) else {
        return recovery(Some(slot.id), RecoveryReason::Unavailable);
    };
    let reason = match slot.state {
        State::Ready => match identity {
            Some(identity) => return Resolution::Ready(slot, Box::new(identity)),
            None => RecoveryReason::Missing,
        },
        State::Pending => RecoveryReason::Incomplete,
        State::Deleting => RecoveryReason::Deleting,
        State::Broken => RecoveryReason::Damaged,
    };
    Resolution::Recovery {
        id: Some(slot.id),
        reason,
        peer_id: identity.map(|identity| identity.peer_id()),
    }
}

fn fallback(mut slots: Vec<Slot>, keys: &dyn KeyReader) -> Resolution {
    if slots.len() == 1 {
        open_slot(slots.remove(0), keys)
    } else {
        Resolution::Picker(slots)
    }
}

fn empty(root: &Path, layout: Layout, keys: &dyn KeyReader) -> Resolution {
    let slot = Slot {
        id: LEGACY.into(),
        dir: root.into(),
        state: State::Ready,
        storage: Some(if layout == Layout::Desktop {
            Storage::System
        } else {
            Storage::Wrapped
        }),
    };
    match keys.contains(&slot) {
        Ok(false) => Resolution::Setup,
        Ok(true) => recovery(Some(slot.id), RecoveryReason::Missing),
        Err(_) => recovery(Some(slot.id), RecoveryReason::Unavailable),
    }
}

/// Resolve the saved pointer or sole candidate, without rewriting any metadata.
/// Caller must serialize this read with registry mutations.
///
/// # Errors
/// Returns filesystem inventory failures; these must never be treated as fresh setup.
pub fn resolve(root: &Path, layout: Layout, keys: &dyn KeyReader) -> Result<Resolution, String> {
    let mut slots = list(root, layout).map_err(|error| error.to_string())?;
    let pointer = active(root, layout);
    match pointer {
        Active::Selected(id) => {
            if let Some(index) = slots.iter().position(|slot| slot.id == id) {
                return Ok(open_slot(slots.remove(index), keys));
            }
            if slots.is_empty() || (layout == Layout::Ios && id == LEGACY) {
                return Ok(recovery(Some(id), RecoveryReason::Missing));
            }
        }
        Active::Invalid if slots.is_empty() => {
            return Ok(recovery(None, RecoveryReason::InvalidPointer));
        }
        Active::Unselected if !slots.is_empty() => return Ok(Resolution::Picker(slots)),
        Active::Absent | Active::Invalid | Active::Unselected => {}
    }
    Ok(if slots.is_empty() {
        empty(root, layout, keys)
    } else {
        fallback(slots, keys)
    })
}
