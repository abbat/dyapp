//! Discovery observations constrain cached results without mutating ordinary profile authority.
use dyapp_identity::SignedRecord;
use dyapp_profile::VerifiedProfile;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Observed(HashMap<String, (u64, bool)>);

impl Observed {
    pub(super) fn observe(&mut self, records: &[SignedRecord]) -> bool {
        let mut valid = true;
        for record in records {
            match dyapp_profile::verify_search(record) {
                Ok(profile) => {
                    let state = (profile.profile.version, profile.profile.deleted);
                    self.0
                        .entry(profile.peer_id)
                        .and_modify(|held| *held = (*held).max(state))
                        .or_insert(state);
                }
                Err(_) => valid = false,
            }
        }
        valid
    }

    pub(super) fn permits(&self, profile: &VerifiedProfile) -> bool {
        self.0.get(&profile.peer_id).is_none_or(|observed| {
            // A withdrawal also suppresses a cached positive at the same conflicting version.
            (profile.profile.version, false) >= *observed
        })
    }
}
