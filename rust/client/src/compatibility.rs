//! Local evidence shared by two verified public profiles.

use dyapp_profile::VerifiedProfile;

/// Matching and comparable dimensions, each from 0 to 6. No comparison is a compatibility claim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Evidence {
    /// Dimensions with the same text or overlapping income ranges.
    pub matches: u8,
    /// Dimensions both profiles published and could be compared.
    pub compared: u8,
}

/// Compare two already verified profiles locally; never send this evidence to a node.
pub fn compare(own: &VerifiedProfile, other: &VerifiedProfile) -> Evidence {
    let (a, b) = (&own.profile, &other.profile);
    let mut evidence = Evidence::default();
    for (left, right) in [
        (&a.profession, &b.profession),
        (&a.company, &b.company),
        (&a.industry, &b.industry),
        (&a.career_stage, &b.career_stage),
        (&a.work_life_balance, &b.work_life_balance),
    ] {
        if !left.is_empty() && !right.is_empty() {
            evidence.compared += 1;
            evidence.matches += u8::from(left == right);
        }
    }
    if !a.country.is_empty() && a.country == b.country {
        if let (Some(a_from), Some(a_to), Some(b_from), Some(b_to)) =
            (a.income_from, a.income_to, b.income_from, b.income_to)
        {
            evidence.compared += 1;
            evidence.matches += u8::from(a_from <= b_to && b_from <= a_to);
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
}
