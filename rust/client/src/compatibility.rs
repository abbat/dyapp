//! Local evidence shared by two verified public profiles.

use dyapp_profile::{normalize_interest, Profile, VerifiedProfile};
use std::collections::HashSet;

/// Matching and comparable dimensions, each from 0 to 9. No comparison is a compatibility claim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Evidence {
    /// Dimensions with the same value, overlapping income ranges or a shared interest.
    pub matches: u8,
    /// Dimensions both profiles published and could be compared.
    pub compared: u8,
}

/// Compare two already verified profiles locally; never send this evidence to a node.
pub fn compare(own: &VerifiedProfile, other: &VerifiedProfile) -> Evidence {
    evidence(&own.profile, &other.profile)
}

/// Order verified results by `matches` with the user's own (possibly unpublished) profile,
/// highest first, keeping the received order on ties. The order never leaves the device.
pub fn rank(own: &Profile, results: &mut [VerifiedProfile]) {
    results.sort_by_cached_key(|other| std::cmp::Reverse(evidence(own, &other.profile).matches));
}

fn evidence(a: &Profile, b: &Profile) -> Evidence {
    let mut evidence = Evidence::default();
    let mut add = |matched: bool| {
        evidence.compared += 1;
        evidence.matches += u8::from(matched);
    };
    if a.relationship_goal != 0 && b.relationship_goal != 0 {
        add(a.relationship_goal == b.relationship_goal);
    }
    if let (Some(left), Some(right)) = (a.wants_more_kids, b.wants_more_kids) {
        add(left == right);
    }
    if !a.interests.is_empty() && !b.interests.is_empty() {
        let own: HashSet<_> = a.interests.iter().map(|i| normalize_interest(i)).collect();
        // Verified interests are already normalized; the own draft may not be.
        add(b.interests.iter().any(|i| own.contains(i)));
    }
    for (left, right) in [
        (&a.profession, &b.profession),
        (&a.company, &b.company),
        (&a.industry, &b.industry),
        (&a.career_stage, &b.career_stage),
        (&a.work_life_balance, &b.work_life_balance),
    ] {
        if !left.is_empty() && !right.is_empty() {
            add(left == right);
        }
    }
    if !a.country.is_empty() && a.country == b.country {
        if let (Some(a_from), Some(a_to), Some(b_from), Some(b_to)) =
            (a.income_from, a.income_to, b.income_from, b.income_to)
        {
            add(a_from <= b_to && b_from <= a_to);
        }
    }
    evidence
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;
    use dyapp_profile::{verify, Profile};

    fn verified(profile: Profile) -> VerifiedProfile {
        verify(&profile.sign(&Identity::generate())).unwrap()
    }

    #[test]
    fn each_optional_text_field_contributes_only_when_both_publish_it() {
        let own = verified(Profile {
            version: 1,
            profession: "Engineer".into(),
            company: "Example".into(),
            industry: "Technology".into(),
            career_stage: "Senior".into(),
            work_life_balance: "Flexible".into(),
            ..Profile::default()
        });
        let mut other = Profile {
            version: 1,
            ..Profile::default()
        };
        assert_eq!(compare(&own, &verified(other.clone())), Evidence::default());
        for (field, value) in ["Engineer", "Example", "Technology", "Senior", "Flexible"]
            .into_iter()
            .enumerate()
        {
            *[
                &mut other.profession,
                &mut other.company,
                &mut other.industry,
                &mut other.career_stage,
                &mut other.work_life_balance,
            ][field] = value.into();
            let count = u8::try_from(field + 1).unwrap();
            assert_eq!(
                compare(&own, &verified(other.clone())),
                Evidence {
                    matches: count,
                    compared: count
                }
            );
        }
        other.company = "Another".into();
        assert_eq!(
            compare(&own, &verified(other)),
            Evidence {
                matches: 4,
                compared: 5
            }
        );
    }

    #[test]
    fn income_requires_complete_ranges_in_the_same_country() {
        let own = verified(Profile {
            version: 1,
            country: "DE".into(),
            income_from: Some(100),
            income_to: Some(200),
            ..Profile::default()
        });
        let mut other = Profile {
            version: 1,
            country: "DE".into(),
            income_from: Some(200),
            income_to: Some(300),
            ..Profile::default()
        };
        assert_eq!(
            compare(&own, &verified(other.clone())),
            Evidence {
                matches: 1,
                compared: 1
            }
        );
        other.income_from = Some(201);
        assert_eq!(
            compare(&own, &verified(other.clone())),
            Evidence {
                matches: 0,
                compared: 1
            }
        );
        other.country = "FR".into();
        assert_eq!(compare(&own, &verified(other.clone())), Evidence::default());
        other.country.clear();
        assert_eq!(compare(&own, &verified(other.clone())), Evidence::default());
        other.country = "DE".into();
        other.income_to = None;
        assert_eq!(compare(&own, &verified(other)), Evidence::default());
    }

    #[test]
    fn goal_kids_and_interests_count_and_rank_keeps_ties_in_order() {
        let own = Profile {
            version: 1,
            relationship_goal: 2,
            wants_more_kids: Some(true),
            interests: vec!["Hiking".into(), "chess".into()],
            ..Profile::default()
        };
        let unset = verified(Profile {
            version: 1,
            ..Profile::default()
        });
        let differs = verified(Profile {
            version: 1,
            relationship_goal: 3,
            wants_more_kids: Some(false),
            interests: vec!["films".into()],
            ..Profile::default()
        });
        let same = verified(Profile {
            version: 1,
            relationship_goal: 2,
            wants_more_kids: Some(true),
            interests: vec!["hiking".into()],
            ..Profile::default()
        });
        // Published interests are normalized; the own draft may not be yet.
        let own_verified = verified(Profile {
            interests: vec!["hiking".into(), "chess".into()],
            ..own.clone()
        });
        assert_eq!(compare(&own_verified, &unset), Evidence::default());
        assert_eq!(
            compare(&own_verified, &differs),
            Evidence {
                matches: 0,
                compared: 3
            }
        );
        assert_eq!(
            compare(&own_verified, &same),
            Evidence {
                matches: 3,
                compared: 3
            }
        );
        let mut results = vec![unset.clone(), same.clone(), differs.clone()];
        rank(&own, &mut results);
        let order: Vec<_> = results.iter().map(|r| r.peer_id.clone()).collect();
        assert_eq!(order, [same.peer_id, unset.peer_id, differs.peer_id]);
    }
}
