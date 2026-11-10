//! Offered categorical and selected-location choices, independent of client UI.

use dyapp_p2p_net::proto;
use std::collections::BTreeMap;

/// Raw categorical choices and selected text/location fields from a search form.
#[derive(Clone, Debug, Default)]
pub struct SearchChoiceInputs {
    /// Scale key to chosen wire value; zero means no constraint.
    pub picked: BTreeMap<String, i64>,
    /// Country code, rather than its translated display label.
    pub country: String,
    /// Optional interest text.
    pub interest: String,
    /// An actual selected suggestion; free-form place text is not a selection.
    pub place: Option<u32>,
}

impl SearchChoiceInputs {
    /// Build equality conditions only for offered fields, in canonical scale order.
    ///
    /// Country, interest and place are independent offered fields. The caller's picker
    /// invalidates its selected ID when the country or place text changes. Unknown scale
    /// keys and unset choices are ignored; existing nonzero wire values are preserved.
    #[must_use]
    pub fn conditions(&self, offered: &[u32]) -> Vec<proto::Condition> {
        let mut out: Vec<_> = dyapp_profile::SCALES
            .iter()
            .filter(|scale| offered.contains(&scale.field))
            .filter_map(|scale| {
                self.picked
                    .get(scale.key)
                    .filter(|code| **code != 0)
                    .map(|code| proto::Condition {
                        field: scale.field,
                        op: proto::Op::Eq.into(),
                        ints: vec![*code],
                        strings: Vec::new(),
                    })
            })
            .collect();
        append_text(&mut out, offered, 4, &self.country);
        let interest = dyapp_profile::normalize_interest(super::trim(&self.interest));
        append_text(&mut out, offered, 19, &interest);
        if let Some(place) = self.place {
            append_text(&mut out, offered, 20, &place.to_string());
        }
        out
    }
}

fn append_text(out: &mut Vec<proto::Condition>, offered: &[u32], field: u32, raw: &str) {
    let text = super::trim(raw);
    if offered.contains(&field) && !text.is_empty() {
        out.push(proto::Condition {
            field,
            op: proto::Op::Eq.into(),
            ints: Vec::new(),
            strings: vec![text.into()],
        });
    }
}

#[cfg(test)]
mod tests;
