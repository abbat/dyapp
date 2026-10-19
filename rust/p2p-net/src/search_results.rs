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
            let keep = !p.deleted
                && conditions.iter().all(|c| matches(c, p))
                && p.pow_bits >= min_pow_bits
                && (p.pow_bits == 0
                    || dyapp_profile::pow_leading_bits(&owner, p.pow_nonce) >= p.pow_bits);
            keep.then_some(verified)
        })
        .collect()
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
        12 => text(&profile.education),
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
    }
}
