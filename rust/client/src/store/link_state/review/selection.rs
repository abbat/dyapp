//! Complete, explicit choices produce a preview only, never import or enrollment authority.
use super::{LinkPreference, LinkPreferenceValue, LinkStateReview};
use std::collections::{BTreeMap, BTreeSet};

/// The owner-selected source of a differing private preference.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LinkPreferenceSource {
    /// Keep the frozen current local value.
    Local,
    /// Use the value from the frozen pending snapshot.
    Pending,
}

/// An explicit decision for one offered owner preference.
#[derive(Clone)]
pub struct LinkPreferenceChoice {
    /// None only for account-wide Hidden.
    pub peer: Option<String>,
    /// An owner-controlled field offered by this review.
    pub field: LinkPreference,
    /// The explicitly selected snapshot.
    pub source: LinkPreferenceSource,
}

/// A selected value for preview; contains no historical clock, actor or event identifier.
pub struct LinkSelectedPreference {
    peer: Option<String>,
    field: LinkPreference,
    value: LinkPreferenceValue,
}

impl LinkSelectedPreference {
    /// The peer, or None for account-wide Hidden.
    #[must_use]
    pub fn peer(&self) -> Option<&str> {
        self.peer.as_deref()
    }

    /// Selected owner-controlled field.
    #[must_use]
    pub fn field(&self) -> LinkPreference {
        self.field
    }

    /// Selected frozen value.
    #[must_use]
    pub fn value(&self) -> &LinkPreferenceValue {
        &self.value
    }
}

/// Invalid or incomplete owner choices; neither snapshot has changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LinkPreferenceSelectionError {
    /// Every differing owner preference needs a decision.
    #[error("choose a source for every differing private preference")]
    Incomplete,
    /// A preference has more than one decision.
    #[error("a private preference was selected more than once")]
    Duplicate,
    /// The submitted peer/field is not offered by the frozen comparison.
    #[error("the private preference is not offered by this review")]
    UnexpectedField,
}

impl LinkStateReview {
    /// Validate complete choices and return a preview in the core's stable presentation order.
    /// A preview does not change state, resolve protected evidence or authorize link completion.
    /// Confirmation must recompute choices and revalidate owner/admission/context under its writer.
    ///
    /// # Errors
    /// Refuses missing, duplicate or unoffered decisions, including decisions for equal fields.
    pub fn select_preferences(
        &self,
        choices: &[LinkPreferenceChoice],
    ) -> Result<Vec<LinkSelectedPreference>, LinkPreferenceSelectionError> {
        let differences = self.differences();
        let offered: BTreeSet<_> = differences
            .iter()
            .map(|difference| (difference.peer.clone(), difference.field))
            .collect();
        let mut selected = BTreeMap::new();
        for choice in choices {
            let key = (choice.peer.clone(), choice.field);
            if !offered.contains(&key) {
                return Err(LinkPreferenceSelectionError::UnexpectedField);
            }
            if selected.insert(key, choice.source).is_some() {
                return Err(LinkPreferenceSelectionError::Duplicate);
            }
        }
        if selected.len() != offered.len() {
            return Err(LinkPreferenceSelectionError::Incomplete);
        }
        differences
            .into_iter()
            .map(|difference| {
                let key = (difference.peer.clone(), difference.field);
                let value = match selected
                    .get(&key)
                    .ok_or(LinkPreferenceSelectionError::Incomplete)?
                {
                    LinkPreferenceSource::Local => difference.local,
                    LinkPreferenceSource::Pending => difference.pending,
                };
                Ok(LinkSelectedPreference {
                    peer: difference.peer,
                    field: difference.field,
                    value,
                })
            })
            .collect()
    }
}
