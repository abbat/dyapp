//! Stored read-coverage snapshots, independent of UI or HLC display ordering.

use std::collections::{HashMap, HashSet};

/// Exact own-message keys admitted by a Store read-marker snapshot.
#[derive(Debug)]
pub struct ReadCoverage {
    own: String,
    groups: HashMap<Vec<u8>, HashSet<[u8; 16]>>,
}

impl ReadCoverage {
    pub(super) fn new(own: &str, keys: impl IntoIterator<Item = (Vec<u8>, [u8; 16])>) -> Self {
        let mut groups = HashMap::<_, HashSet<_>>::new();
        for (group, id) in keys {
            groups.entry(group).or_default().insert(id);
        }
        Self {
            own: own.to_owned(),
            groups,
        }
    }

    /// Read flag for an exact own key; foreign-author flags must remain untouched.
    /// Edited/deleted rows and malformed or absent IDs never inherit a previous read.
    #[must_use]
    pub fn read_flag(
        &self,
        group: &[u8],
        author: &str,
        id: &[u8],
        edited: bool,
        deleted: bool,
    ) -> Option<bool> {
        if author != self.own {
            return None;
        }
        let known = <[u8; 16]>::try_from(id)
            .is_ok_and(|id| self.groups.get(group).is_some_and(|ids| ids.contains(&id)));
        Some(known && !edited && !deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_group_author_and_logical_id_are_required() {
        let covered = ReadCoverage::new("me", [(vec![1], [3; 16]), (vec![2], [4; 16])]);
        assert_eq!(
            covered.read_flag(&[1], "me", &[3; 16], false, false),
            Some(true)
        );
        assert_eq!(
            covered.read_flag(&[2], "me", &[3; 16], false, false),
            Some(false)
        );
        assert_eq!(
            covered.read_flag(&[1], "peer", &[3; 16], false, false),
            None
        );
        assert_eq!(
            covered.read_flag(&[1], "me", &[4; 16], false, false),
            Some(false)
        );
    }

    #[test]
    fn edited_deleted_missing_and_malformed_rows_cannot_inherit_a_read() {
        let covered = ReadCoverage::new("me", [(vec![1], [3; 16])]);
        for (edited, deleted) in [(true, false), (false, true), (true, true)] {
            assert_eq!(
                covered.read_flag(&[1], "me", &[3; 16], edited, deleted),
                Some(false)
            );
        }
        assert_eq!(
            covered.read_flag(&[1], "me", &[3], false, false),
            Some(false)
        );
        assert_eq!(
            covered.read_flag(&[9], "me", &[3; 16], false, false),
            Some(false)
        );
        let empty = ReadCoverage::new("me", []);
        assert_eq!(
            empty.read_flag(&[1], "me", &[3; 16], false, false),
            Some(false)
        );
    }
}
