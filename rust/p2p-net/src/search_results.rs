//! Client-side check of search results: a search node is untrusted, so the client re-applies
//! its own conditions to every record it gets and merges the replies of several nodes.

use crate::proto::{Condition, Op};
use dyapp_identity::SignedRecord;
use dyapp_profile::{Profile, VerifiedProfile};
use std::collections::HashMap;

/// The profiles in `replies` (one list per search node) that match every condition, one per
/// owner, in first-seen order. Of each owner only the highest validly signed version counts, so
/// a node cannot bring back an older matching version; it is dropped when it is a tombstone,
/// misses a condition or lacks a profile proof of work of `min_pow_bits` that the Argon2 hash
/// confirms. A condition the node would skip (unknown field or op, `range` on a string field)
/// is skipped here too; a malformed one matches nothing.
pub fn check(
    conditions: &[Condition],
    min_pow_bits: u32,
    replies: &[Vec<SignedRecord>],
) -> Vec<VerifiedProfile> {
    let mut order = Vec::new();
    let mut newest: HashMap<[u8; 32], VerifiedProfile> = HashMap::new();
    for record in replies.iter().flatten() {
        let Ok(verified) = dyapp_profile::verify(record) else {
            continue;
        };
        // A verified record has a valid 32-byte key.
        let owner: [u8; 32] = record.public_key.as_slice().try_into().unwrap_or_default();
        match newest.get(&owner) {
            None => order.push(owner),
            Some(v) if v.profile.version >= verified.profile.version => continue,
            Some(_) => {}
        }
        newest.insert(owner, verified);
    }
    order
        .into_iter()
        .filter_map(|owner| {
            let verified = newest.remove(&owner)?;
            let p = &verified.profile;
            // Argon2 last: the cheap checks drop most records first.
            let keep = passes(conditions, min_pow_bits, p)
                && (p.pow_bits == 0
                    || dyapp_profile::pow_leading_bits(&owner, p.pow_nonce) >= p.pow_bits);
            keep.then_some(verified)
        })
        .collect()
}

/// Whether `profile` is no tombstone, matches every condition and claims at least `min_pow_bits`
/// of proof of work, as in [`check`], which also confirms the claim with the Argon2 hash.
pub fn passes(conditions: &[Condition], min_pow_bits: u32, profile: &Profile) -> bool {
    !profile.deleted
        && conditions.iter().all(|c| matches(c, profile))
        && profile.pow_bits >= min_pow_bits
}

/// A profile field as the search index stores it: integers, strings, or absent (SQL NULL).
enum Value<'a> {
    Ints(Vec<i64>),
    Strings(Vec<&'a str>),
}

/// Field numbers as in `search::COLUMNS` on the node; `None` for a field queries cannot use.
fn value(profile: &Profile, field: u32) -> Option<Value<'_>> {
    fn text(v: &str) -> Option<Value<'_>> {
        Some(Value::Strings(vec![v]))
    }
    let int = |v: Option<i64>| Some(Value::Ints(v.into_iter().collect()));
    let wide = |v: Option<u64>| int(v.map(|v| i64::try_from(v).unwrap_or(i64::MAX)));
    match field {
        3 => int(Some(profile.age.into())),
        4 => text(&profile.country),
        6 => wide(profile.income_from),
        7 => wide(profile.income_to),
        8 => int(profile.has_kids.map(i64::from)),
        9 => int(profile.wants_more_kids.map(i64::from)),
        10 => text(&profile.relationship_goal),
        11 => text(&profile.career_ambition),
        13 => text(&profile.orientation),
        14 => text(&profile.role_preference),
        15 => text(&profile.zodiac),
        16 => text(&profile.financial_philosophy),
        17 => text(&profile.employment_status),
        18 => text(&profile.fitness_level),
        19 => Some(Value::Strings(
            profile.interests.iter().map(String::as_str).collect(),
        )),
        20 => text(&profile.place),
        21 => int(i64::try_from(profile.photos.len()).ok()),
        25 => text(&profile.gender),
        27 => int(Some(profile.education_level.into())),
        _ => None,
    }
}

fn matches(c: &Condition, profile: &Profile) -> bool {
    let op = Op::try_from(c.op).unwrap_or(Op::Unspecified);
    let Some(value) = value(profile, c.field) else {
        return true;
    };
    match value {
        Value::Ints(have) => {
            if !c.strings.is_empty() {
                return false;
            }
            let wants = &c.ints;
            match op {
                Op::Unspecified => true,
                Op::Eq if wants.len() == 1 => have.contains(&wants[0]),
                Op::In if (1..=8).contains(&wants.len()) => have.iter().any(|v| wants.contains(v)),
                Op::Range if wants.len() == 2 && wants[0] <= wants[1] => {
                    have.iter().any(|v| (wants[0]..=wants[1]).contains(v))
                }
                _ => false,
            }
        }
        Value::Strings(have) => {
            if !c.ints.is_empty() {
                return false;
            }
            let wants = &c.strings;
            match op {
                Op::Unspecified | Op::Range => true,
                Op::Eq if wants.len() == 1 => have.contains(&wants[0].as_str()),
                Op::In if (1..=8).contains(&wants.len()) => {
                    have.iter().any(|v| wants.iter().any(|w| w == v))
                }
                _ => false,
            }
        }
    }
}

/// What a person searches for, turned into node conditions by [`Filter::conditions`]. An empty
/// list or `None` leaves that field open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    /// Inclusive; the lower bound is raised to 18.
    pub age: Option<(u32, u32)>,
    /// Any of these `gender` values.
    pub genders: Vec<String>,
    /// Any of these `orientation` values.
    pub orientations: Vec<String>,
    /// Any of these `relationship_goal` values.
    pub relationship_goals: Vec<String>,
    /// `has_kids` as stated; a profile that leaves it unset does not match.
    pub has_kids: Option<bool>,
    /// `wants_more_kids` as stated; a profile that leaves it unset does not match.
    pub wants_more_kids: Option<bool>,
    /// ISO 3166-1 alpha-2 code, or empty for any country.
    pub country: String,
    /// GeoNames IDs as profiles state them; needs `country`.
    pub places: Vec<String>,
    /// Inclusive; matches a profile whose own income range overlaps it. Needs `country`.
    pub income: Option<(u64, u64)>,
    /// Least `education_level`, from 1 to 6; a profile that leaves it unset does not match.
    pub min_education: Option<u32>,
    /// Only profiles with at least one photo.
    pub with_photo: bool,
}

/// Why [`Filter::conditions`] refused a filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterError {
    /// More than 8 values for one field.
    TooManyValues,
    /// Places or income without a country.
    NeedsCountry,
    /// A range whose lower bound is above its upper one.
    EmptyRange,
}

impl Filter {
    /// The conditions for a query and for [`check`]: at most 12, one per field.
    pub fn conditions(&self) -> Result<Vec<Condition>, FilterError> {
        if self.country.is_empty() && (!self.places.is_empty() || self.income.is_some()) {
            return Err(FilterError::NeedsCountry);
        }
        let ints = |field, op: Op, ints: Vec<i64>| Condition {
            field,
            op: op.into(),
            ints,
            strings: Vec::new(),
        };
        let wide = |v: u64| i64::try_from(v).unwrap_or(i64::MAX);
        let mut out = Vec::new();
        if let Some((min, max)) = self.age {
            let min = min.max(18);
            if min > max {
                return Err(FilterError::EmptyRange);
            }
            out.push(ints(3, Op::Range, vec![min.into(), max.into()]));
        }
        if !self.country.is_empty() {
            out.push(Condition {
                field: 4,
                op: Op::Eq.into(),
                ints: Vec::new(),
                strings: vec![self.country.clone()],
            });
        }
        if let Some((min, max)) = self.income {
            if min > max {
                return Err(FilterError::EmptyRange);
            }
            out.push(ints(6, Op::Range, vec![0, wide(max)]));
            out.push(ints(7, Op::Range, vec![wide(min), i64::MAX]));
        }
        if let Some(min) = self.min_education {
            if !(1..=6).contains(&min) {
                return Err(FilterError::EmptyRange);
            }
            out.push(ints(27, Op::Range, vec![min.into(), 6]));
        }
        if self.with_photo {
            out.push(ints(21, Op::Range, vec![1, i64::MAX]));
        }
        for (field, v) in [(8, self.has_kids), (9, self.wants_more_kids)] {
            if let Some(v) = v {
                out.push(ints(field, Op::Eq, vec![v.into()]));
            }
        }
        let lists = [
            (10, &self.relationship_goals),
            (13, &self.orientations),
            (20, &self.places),
            (25, &self.genders),
        ];
        for (field, values) in lists {
            if values.len() > 8 {
                return Err(FilterError::TooManyValues);
            }
            if !values.is_empty() {
                out.push(Condition {
                    field,
                    op: Op::In.into(),
                    ints: Vec::new(),
                    strings: values.clone(),
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;

    fn record(identity: &Identity, version: u64, age: u32, interests: &[&str]) -> SignedRecord {
        Profile {
            version,
            age,
            country: "DE".into(),
            interests: interests.iter().map(|s| (*s).to_string()).collect(),
            ..Profile::default()
        }
        .sign(identity)
    }

    fn condition(field: u32, op: Op, ints: &[i64], strings: &[&str]) -> Condition {
        Condition {
            field,
            op: op.into(),
            ints: ints.to_vec(),
            strings: strings.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn drops_mismatches_and_forgeries_and_merges_two_nodes() {
        let [a, b, c] = [(); 3].map(|()| Identity::generate());
        let mut forged = record(&c, 1, 30, &["chess"]);
        forged.payload = record(&c, 1, 31, &["chess"]).payload;
        let honest = vec![record(&a, 1, 30, &["chess"]), record(&b, 1, 30, &["go"])];
        // A lying node: a mismatch on age, a forged signature, an older and a newer version.
        let lying = vec![
            record(&b, 2, 50, &["chess"]),
            forged,
            record(&a, 3, 33, &["chess", "go"]),
            record(&a, 2, 31, &["chess"]),
        ];
        let conditions = [
            condition(3, Op::Range, &[25, 35], &[]),
            condition(19, Op::In, &[], &["chess", "ski"]),
            condition(4, Op::Eq, &[], &["DE"]),
            condition(99, Op::Eq, &[1], &[]),
        ];
        let found = check(&conditions, 0, &[honest, lying]);
        let ages: Vec<_> = found.iter().map(|v| v.profile.age).collect();
        assert_eq!(ages, [33]);
        assert_eq!(found[0].peer_id, a.peer_id());
        // A newer version that misses the conditions hides an older one that matches.
        let newer = vec![record(&a, 4, 60, &["chess"])];
        let older = vec![record(&a, 1, 30, &["chess"])];
        assert!(check(&conditions, 0, &[older, newer]).is_empty());
    }

    #[test]
    fn malformed_conditions_match_nothing_and_pow_is_checked() {
        let a = Identity::generate();
        let replies = [vec![record(&a, 1, 30, &[])]];
        let bad = condition(3, Op::Eq, &[], &["30"]);
        assert!(check(&[bad], 0, &replies).is_empty());
        let reversed = condition(3, Op::Range, &[40, 20], &[]);
        assert!(check(&[reversed], 0, &replies).is_empty());
        assert_eq!(check(&[], 0, &replies).len(), 1);
        assert!(check(&[], 1, &replies).is_empty());
        // Claimed bits the Argon2 hash does not reach.
        let lying = Profile {
            version: 1,
            pow_bits: 30,
            ..Profile::default()
        }
        .sign(&a);
        assert!(check(&[], 0, &[vec![lying]]).is_empty());
        let key: [u8; 32] = replies[0][0].public_key.as_slice().try_into().unwrap();
        let nonce = dyapp_profile::solve(&key, 2, 0, 1000).unwrap();
        let mined = Profile {
            version: 1,
            pow_bits: 2,
            pow_nonce: nonce,
            ..Profile::default()
        }
        .sign(&a);
        assert_eq!(check(&[], 2, &[vec![mined]]).len(), 1);
    }

    #[test]
    fn every_field_and_op_is_checked() {
        let p = Profile {
            age: 30,
            country: "DE".into(),
            income_from: Some(10),
            income_to: Some(u64::MAX),
            has_kids: Some(true),
            wants_more_kids: Some(false),
            relationship_goal: "f10".into(),
            career_ambition: "f11".into(),
            orientation: "f13".into(),
            role_preference: "f14".into(),
            zodiac: "f15".into(),
            financial_philosophy: "f16".into(),
            employment_status: "f17".into(),
            fitness_level: "f18".into(),
            interests: vec!["chess".into(), "go".into()],
            place: "f20".into(),
            gender: "f25".into(),
            education_level: 4,
            photos: vec![dyapp_profile::Photo::default()],
            ..Profile::default()
        };
        let hits = |field, op, ints: &[i64], strings: &[&str]| {
            matches(&condition(field, op, ints, strings), &p)
        };
        for (field, v) in [
            (3, 30),
            (6, 10),
            (7, i64::MAX),
            (8, 1),
            (9, 0),
            (21, 1),
            (27, 4),
        ] {
            assert!(hits(field, Op::Eq, &[v], &[]), "{field}");
            assert!(!hits(field, Op::Eq, &[v - 1], &[]), "{field}");
        }
        let texts = [10, 11]
            .into_iter()
            .chain(13..=18)
            .chain([20, 25])
            .map(|f| (f, format!("f{f}")));
        let fixed = [(4, "DE".to_string()), (19, "go".into())];
        for (field, v) in fixed.into_iter().chain(texts) {
            assert!(hits(field, Op::Eq, &[], &[&v]), "{field}");
            assert!(!hits(field, Op::Eq, &[], &["x"]), "{field}");
        }
        let nine = [22, 23, 24, 25, 26, 27, 28, 29, 30];
        assert!(hits(3, Op::Unspecified, &[], &[]));
        assert!(!hits(3, Op::Eq, &[30, 31], &[]));
        assert!(hits(3, Op::In, &[29, 30], &[]));
        assert!(!hits(3, Op::In, &[], &[]));
        assert!(!hits(3, Op::In, &nine, &[]));
        assert!(hits(3, Op::Range, &[20, 40], &[]));
        assert!(!hits(3, Op::Range, &[20, 40, 50], &[]));
        let nine = ["a", "b", "c", "d", "e", "f", "g", "h", "DE"];
        assert!(hits(4, Op::Unspecified, &[], &[]));
        assert!(hits(4, Op::Range, &[], &[]));
        assert!(!hits(4, Op::Eq, &[], &["DE", "FR"]));
        assert!(hits(4, Op::In, &[], &["FR", "DE"]));
        assert!(!hits(4, Op::In, &[], &["FR"]));
        assert!(!hits(4, Op::In, &[], &nine));
    }

    #[test]
    fn a_filter_keeps_the_matching_profile_and_drops_each_mismatch() {
        let a = Identity::generate();
        let fit = Profile {
            version: 1,
            age: 30,
            gender: "f".into(),
            orientation: "straight".into(),
            relationship_goal: "family".into(),
            has_kids: Some(false),
            wants_more_kids: Some(true),
            country: "DE".into(),
            place: "2950159".into(),
            income_from: Some(40_000),
            income_to: Some(60_000),
            education_level: 4,
            photos: vec![dyapp_profile::Photo {
                full: vec![vec![1; 32]],
                thumbnail: vec![2; 32],
            }],
            ..Profile::default()
        };
        let strings = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let filter = Filter {
            age: Some((0, 35)),
            genders: strings(&["f", "nb"]),
            orientations: strings(&["straight"]),
            relationship_goals: strings(&["family", "long"]),
            has_kids: Some(false),
            wants_more_kids: Some(true),
            country: "DE".into(),
            places: strings(&["2950159"]),
            income: Some((55_000, u64::MAX)),
            min_education: Some(3),
            with_photo: true,
        };
        let conditions = filter.conditions().unwrap();
        assert_eq!(conditions.len(), 12);
        assert_eq!(conditions[0].ints, [18, 35]);
        let kept = |p: &Profile| check(&conditions, 0, &[vec![p.sign(&a)]]).len() == 1;
        assert!(kept(&fit));
        let misses: [fn(&mut Profile); 12] = [
            |p| p.age = 36,
            |p| p.gender = "m".into(),
            |p| p.orientation = "gay".into(),
            |p| p.relationship_goal = "casual".into(),
            |p| p.has_kids = Some(true),
            |p| p.wants_more_kids = None,
            |p| p.country = "FR".into(),
            |p| p.place = "2867714".into(),
            |p| p.income_to = Some(50_000),
            |p| p.income_from = None,
            |p| p.education_level = 0,
            |p| p.photos.clear(),
        ];
        for (i, miss) in misses.iter().enumerate() {
            let mut p = fit.clone();
            miss(&mut p);
            assert!(!kept(&p), "{i}");
        }
        // The income ranges overlap at one end only.
        let mut p = fit.clone();
        p.income_to = Some(55_000);
        assert!(kept(&p));
        assert_eq!(Filter::default().conditions().unwrap(), []);
    }

    #[test]
    fn a_filter_refuses_what_a_node_would_not_take() {
        let open = |filter: Filter| filter.conditions().unwrap_err();
        let places = Filter {
            places: vec!["1".into()],
            ..Filter::default()
        };
        assert_eq!(open(places), FilterError::NeedsCountry);
        let income = Filter {
            income: Some((1, 2)),
            ..Filter::default()
        };
        assert_eq!(open(income), FilterError::NeedsCountry);
        let young = Filter {
            age: Some((10, 17)),
            ..Filter::default()
        };
        assert_eq!(open(young), FilterError::EmptyRange);
        let poor = Filter {
            country: "DE".into(),
            income: Some((2, 1)),
            ..Filter::default()
        };
        assert_eq!(open(poor), FilterError::EmptyRange);
        for min in [0, 7] {
            let level = Filter {
                min_education: Some(min),
                ..Filter::default()
            };
            assert_eq!(open(level), FilterError::EmptyRange);
        }
        let many = Filter {
            genders: (0..9).map(|i| i.to_string()).collect(),
            ..Filter::default()
        };
        assert_eq!(open(many), FilterError::TooManyValues);
        let eight = Filter {
            genders: (0..8).map(|i| i.to_string()).collect(),
            age: Some((18, 18)),
            country: "DE".into(),
            income: Some((1, 1)),
            ..Filter::default()
        };
        assert_eq!(eight.conditions().unwrap().len(), 5);
    }
}
