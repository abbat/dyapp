//! Frozen private-state comparison; preparing or viewing it authorizes no import or enrollment.
use super::{progress, Error, LinkState, Store};
use dyapp_identity::Identity;
use prost::Message as _;
use rusqlite::{Transaction, TransactionBehavior};
use std::collections::{BTreeMap, BTreeSet};

#[path = "review/selection.rs"]
mod selection;
pub use selection::{
    LinkPreferenceChoice, LinkPreferenceSelectionError, LinkPreferenceSource,
    LinkSelectedPreference,
};

/// Owner-controlled preference, never a trust observation or another person's event.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LinkPreference {
    /// Block this peer.
    Blocked,
    /// Mute this peer.
    Muted,
    /// Favorite this peer.
    Favorite,
    /// Local contact name.
    Name,
    /// Private contact note.
    Note,
    /// Pass this peer.
    Passed,
    /// Search visibility.
    Hidden,
    /// The owner's outgoing like, not an incoming like.
    OutgoingLike,
}

/// A frozen value; selecting it must not copy historical actors, clocks or event identifiers.
#[derive(Clone, PartialEq, Eq)]
pub enum LinkPreferenceValue {
    /// Boolean preference.
    Flag(bool),
    /// Local name or note.
    Text(String),
}

/// One differing owner-controlled preference in stable peer/field order.
pub struct LinkPreferenceDifference {
    /// None only for the account-wide Hidden preference.
    pub peer: Option<String>,
    /// The owner-controlled field.
    pub field: LinkPreference,
    /// Current local value.
    pub local: LinkPreferenceValue,
    /// Value in the exact pending snapshot.
    pub pending: LinkPreferenceValue,
}

/// Read-only comparison bound to one owner, database, pending operation and private snapshot.
/// Holds no keys, transaction or identity admission. No Debug implementation exposes private values.
pub struct LinkStateReview {
    owner: String,
    database: String,
    current_bytes: Vec<u8>,
    pending_bytes: Vec<u8>,
    local: LinkState,
    pending: LinkState,
}

impl Store {
    /// Freeze legacy private state requiring recovery, without replaying any private writes.
    /// None means no legacy pending operation needs this comparison.
    ///
    /// # Errors
    /// Refuses another owner, malformed state/progress or unreadable storage.
    /// The caller must separately retain original-device admission during each native operation.
    pub fn prepare_link_state_review(
        &self,
        identity: &Identity,
    ) -> Result<Option<LinkStateReview>, Error> {
        if identity.peer_id() != self.owner_peer_id() {
            return Err(Error::WrongIdentity);
        }
        let snapshot = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let review = self.link_state_review_in_snapshot()?;
        snapshot.commit()?;
        Ok(review)
    }

    fn link_state_review_in_snapshot(&self) -> Result<Option<LinkStateReview>, Error> {
        let Some(pending_bytes) = self.link_pending()? else {
            return Ok(None);
        };
        if progress::read(&self.db, &pending_bytes)?.is_some() {
            return Ok(None);
        }
        let current_bytes = self.link_state_in_snapshot()?;
        Ok(Some(LinkStateReview {
            owner: self.owner_peer_id().to_owned(),
            database: self.db.path().ok_or(Error::InvalidSetting)?.to_owned(),
            local: decode(&current_bytes)?,
            pending: decode(&pending_bytes)?,
            current_bytes,
            pending_bytes,
        }))
    }
}

impl LinkStateReview {
    /// Check the frozen context before a later operation. This is not write authorization.
    ///
    /// # Errors
    /// Refuses another owner or unreadable/malformed storage.
    pub fn is_current(&self, store: &Store, identity: &Identity) -> Result<bool, Error> {
        if identity.peer_id() != self.owner || store.owner_peer_id() != self.owner {
            return Err(Error::WrongIdentity);
        }
        if store.db.path() != Some(self.database.as_str()) {
            return Ok(false);
        }
        let snapshot = Transaction::new_unchecked(&store.db, TransactionBehavior::Deferred)?;
        let same = store.link_pending()?.as_deref() == Some(self.pending_bytes.as_slice())
            && progress::read(&store.db, &self.pending_bytes)?.is_none()
            && store.link_state_in_snapshot()? == self.current_bytes;
        snapshot.commit()?;
        Ok(same)
    }

    /// Compare owner values only. Future confirmation must validate choices, context and each
    /// field's domain policy under its writer. A comparison never authorizes event-history changes.
    #[must_use]
    pub fn differences(&self) -> Vec<LinkPreferenceDifference> {
        let local = preferences(&self.local, &self.owner);
        let pending = preferences(&self.pending, &self.owner);
        let keys: BTreeSet<_> = local.keys().chain(pending.keys()).cloned().collect();
        keys.into_iter()
            .filter_map(|(peer, field)| {
                let key = (peer.clone(), field);
                let local = local
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| default_value(field));
                let pending = pending
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| default_value(field));
                (local != pending).then_some(LinkPreferenceDifference {
                    peer,
                    field,
                    local,
                    pending,
                })
            })
            .collect()
    }

    /// Protected observations/metadata differ; these must never become owner-editable choices.
    /// Includes trust/device observations, incoming events, actor/clock/event metadata and
    /// noncanonical/unknown pending wire data. No merge or consent is implied by this signal.
    #[must_use]
    pub fn has_protected_differences(&self) -> bool {
        self.pending.encode_to_vec() != self.pending_bytes
            || protected(&self.local) != protected(&self.pending)
    }
}

type Preferences = BTreeMap<(Option<String>, LinkPreference), LinkPreferenceValue>;

fn preferences(state: &LinkState, owner: &str) -> Preferences {
    use LinkPreference as F;
    use LinkPreferenceValue::{Flag, Text};
    let mut values = Preferences::new();
    values.insert((None, F::Hidden), Flag(state.hidden));
    for peer in &state.peers {
        for (field, value) in [
            (F::Blocked, Flag(peer.blocked)),
            (F::Muted, Flag(peer.muted)),
            (F::Favorite, Flag(peer.favorite)),
            (F::Name, Text(peer.local_name.clone())),
            (F::Note, Text(peer.note.clone())),
        ] {
            values.insert((Some(peer.peer_id.clone()), field), value);
        }
    }
    for pass in &state.passes {
        values.insert((Some(pass.peer_id.clone()), F::Passed), Flag(pass.passed));
    }
    for like in state
        .likes
        .iter()
        .filter(|like| like.sender == owner && like.target != owner)
    {
        values.insert(
            (Some(like.target.clone()), F::OutgoingLike),
            Flag(like.liked),
        );
    }
    values
}

fn default_value(field: LinkPreference) -> LinkPreferenceValue {
    match field {
        LinkPreference::Name | LinkPreference::Note => LinkPreferenceValue::Text(String::new()),
        _ => LinkPreferenceValue::Flag(false),
    }
}

fn protected(state: &LinkState) -> LinkState {
    let mut protected = state.clone();
    for peer in &mut protected.peers {
        peer.blocked = false;
        peer.muted = false;
        peer.favorite = false;
        peer.local_name.clear();
        peer.note.clear();
    }
    for pass in &mut protected.passes {
        pass.passed = false;
    }
    // Like rows retain both their value and original event identity: neither is editable evidence.
    protected.hidden = false;
    protected
}

fn decode(bytes: &[u8]) -> Result<LinkState, Error> {
    let state = LinkState::decode(bytes).map_err(|_| Error::InvalidSetting)?;
    let peers: BTreeSet<_> = state.peers.iter().map(|peer| &peer.peer_id).collect();
    let passes: BTreeSet<_> = state.passes.iter().map(|pass| &pass.peer_id).collect();
    let likes: BTreeSet<_> = state
        .likes
        .iter()
        .map(|like| (&like.sender, &like.target))
        .collect();
    if peers.len() != state.peers.len()
        || passes.len() != state.passes.len()
        || likes.len() != state.likes.len()
        || state
            .peers
            .iter()
            .any(|peer| !crate::chat::valid_note(&peer.note))
    {
        return Err(Error::InvalidSetting);
    }
    Ok(state)
}
