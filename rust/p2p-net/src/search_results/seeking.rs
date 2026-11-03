//! The mutual search: who a profile seeks, asked of nodes and re-checked on the client.
use super::{CivilDate, Condition, Filter, Op, Profile, MAX_SEEKING_AGE, MIN_SEEKING_AGE};

/// The conditions of a mutual search for `me`, or `None` when `me` states no known gender or the
/// age at `today` is not exact or not within the ages a profile can seek.
pub fn mutual_conditions(me: &Profile, today: CivilDate) -> Option<Vec<Condition>> {
    let (youngest, oldest) = me.possible_age_range(today)?;
    if youngest != oldest {
        return None;
    }
    let filter = Filter {
        seeks: Some((me.gender, oldest)),
        ..Filter::default()
    };
    filter.conditions(today).ok()
}

pub(super) fn seeking_age_matches(c: &Condition, profile: &Profile, op: Op) -> bool {
    if op == Op::Unspecified {
        return true;
    }
    let ([age], true) = (&c.ints[..], op == Op::Eq && c.strings.is_empty()) else {
        return false;
    };
    if profile.seeking_age_from.is_none() && profile.seeking_age_to.is_none() {
        return false;
    }
    let from = profile.seeking_age_from.unwrap_or(MIN_SEEKING_AGE);
    let to = profile.seeking_age_to.unwrap_or(MAX_SEEKING_AGE);
    (i64::from(from)..=i64::from(to)).contains(age)
}
