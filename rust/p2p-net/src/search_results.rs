//! Client-side check of search results: a search node is untrusted, so the client re-applies
//! its own conditions to every record it gets and merges the replies of several nodes.

use crate::proto::{Condition, Op};
use dyapp_identity::SignedRecord;
use dyapp_profile::{
    scale, stated, CareerAmbition, CivilDate, Drinking, EducationLevel, EmploymentStatus,
    FinancialPhilosophy, FitnessLevel, Gender, MaritalStatus, Orientation, Pets, Profile,
    RelationshipGoal, RolePreference, Smoking, Tattoos, VerifiedProfile, Zodiac, HEIGHT_FIELD,
    MAX_HEIGHT_CM, MAX_SEEKING_AGE, MIN_HEIGHT_CM, MIN_SEEKING_AGE,
};
use std::collections::HashMap;

/// The condition field of the UTC day of a key's last heartbeat or publish on the node. A record
/// does not carry it, so [`check`] skips it and the client trusts the nodes that applied it.
pub const ACTIVE_DAY: u32 = 1000;
/// Current-age interval over public birth-date bounds, rechecked from signed profiles.
pub const BIRTH_AGE: u32 = 1001;
/// `eq` the searcher's gender code: the profile's `seeking_genders` holds it.
pub const SEEKING_GENDERS: u32 = 53;
/// `eq` the searcher's age in years: the profile's seeking ages allow it, one open bound leaving
/// that side unlimited; a profile with neither bound does not match.
pub const SEEKING_AGE: u32 = 54;
/// Maximum values in one `in` condition, shared by client checks and bootstrap execution.
pub const MAX_IN_VALUES: usize = 16;

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

/// Whether [`check`] re-applies a condition on `field`, or, for [`ACTIVE_DAY`], drops the nodes
/// that skip it; any other field a node lists cannot be checked and is not to be offered.
pub fn checkable(field: u32) -> bool {
    field == ACTIVE_DAY
        || field == BIRTH_AGE
        || field == SEEKING_AGE
        || value(&Profile::default(), field).is_some()
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
    let code = |v: u32, maximum: u32| int(stated(v, maximum).map(i64::from));
    match field {
        4 => text(&profile.country),
        6 => wide(profile.income_from),
        7 => wide(profile.income_to),
        8 => int(profile.has_kids.map(i64::from)),
        9 => int(profile.wants_more_kids.map(i64::from)),
        RelationshipGoal::FIELD => code(profile.relationship_goal, RelationshipGoal::MAX),
        CareerAmbition::FIELD => code(profile.career_ambition, CareerAmbition::MAX),
        Orientation::FIELD => code(profile.orientation, Orientation::MAX),
        RolePreference::FIELD => code(profile.role_preference, RolePreference::MAX),
        Zodiac::FIELD => code(profile.zodiac, Zodiac::MAX),
        FinancialPhilosophy::FIELD => code(profile.financial_philosophy, FinancialPhilosophy::MAX),
        EmploymentStatus::FIELD => code(profile.employment_status, EmploymentStatus::MAX),
        FitnessLevel::FIELD => code(profile.fitness_level, FitnessLevel::MAX),
        19 => Some(Value::Strings(
            profile.interests.iter().map(String::as_str).collect(),
        )),
        20 => text(&profile.place),
        21 => int(i64::try_from(profile.photos.len()).ok()),
        Gender::FIELD => code(profile.gender, Gender::MAX),
        EducationLevel::FIELD => code(profile.education_level, EducationLevel::MAX),
        HEIGHT_FIELD => int(profile.height.map(i64::from)),
        Smoking::FIELD => code(profile.smoking, Smoking::MAX),
        Drinking::FIELD => code(profile.drinking, Drinking::MAX),
        Tattoos::FIELD => code(profile.tattoos, Tattoos::MAX),
        MaritalStatus::FIELD => code(profile.marital_status, MaritalStatus::MAX),
        Pets::FIELD => code(profile.pets, Pets::MAX),
        SEEKING_GENDERS => Some(Value::Ints(
            profile
                .seeking_genders
                .iter()
                .copied()
                .map(i64::from)
                .collect(),
        )),
        _ => None,
    }
}

fn birth_age_matches(c: &Condition, profile: &Profile, op: Op) -> bool {
    if op == Op::Unspecified {
        return true;
    }
    if op != Op::Range || c.ints.len() != 2 || !c.strings.is_empty() || c.ints[0] >= c.ints[1] {
        return false;
    }
    let Some(bounds) = profile.birth_bounds() else {
        return false;
    };
    i64::from(bounds.earliest.yyyymmdd()) > c.ints[0]
        && i64::from(bounds.latest.yyyymmdd()) <= c.ints[1]
}

fn matches(c: &Condition, profile: &Profile) -> bool {
    let op = Op::try_from(c.op).unwrap_or(Op::Unspecified);
    if c.field == BIRTH_AGE {
        return birth_age_matches(c, profile, op);
    }
    if c.field == SEEKING_AGE {
        return seeking_age_matches(c, profile, op);
    }
    let Some(value) = value(profile, c.field) else {
        return true;
    };
    match value {
        Value::Ints(have) => {
            if !c.strings.is_empty() {
                return false;
            }
            let wants = &c.ints;
            if let Some(scale) = scale(c.field) {
                if (op == Op::Range && !scale.ordered)
                    || (op != Op::Unspecified && wants.iter().any(|&v| v < 1))
                {
                    return false;
                }
            }
            match op {
                Op::Unspecified => true,
                Op::Eq if wants.len() == 1 => have.contains(&wants[0]),
                Op::In if (1..=MAX_IN_VALUES).contains(&wants.len()) => {
                    have.iter().any(|v| wants.contains(v))
                }
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
                Op::In if (1..=MAX_IN_VALUES).contains(&wants.len()) => {
                    have.iter().any(|v| wants.iter().any(|w| w == v))
                }
                _ => false,
            }
        }
    }
}

/// Selection for an ordered categorical scale.
#[derive(Clone, Debug, PartialEq)]
pub enum ScaleSelection {
    /// Match any selected value.
    Any(Vec<u32>),
    /// Inclusive lower and upper values.
    Range(u32, u32),
}

/// What a person searches for, turned into node conditions by [`Filter::conditions`]. An empty
/// list or `None` leaves that field open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    /// Inclusive; the lower bound is raised to 18.
    pub age: Option<(u32, u32)>,
    /// Any of these `gender` values.
    pub genders: Vec<u32>,
    /// Any of these `orientation` values.
    pub orientations: Vec<u32>,
    /// Any of these `relationship_goal` values.
    pub relationship_goals: Vec<u32>,
    /// Any of these family roles.
    pub role_preferences: Vec<u32>,
    /// Any of these employment statuses.
    pub employment_statuses: Vec<u32>,
    /// Any of these self-declared zodiac signs.
    pub zodiacs: Vec<u32>,
    /// Any of these marital statuses.
    pub marital_statuses: Vec<u32>,
    /// Any of these `pets` values.
    pub pets: Vec<u32>,
    /// Selected career ambitions or an inclusive range.
    pub career_ambitions: Option<ScaleSelection>,
    /// Selected financial philosophies or an inclusive range.
    pub financial_philosophies: Option<ScaleSelection>,
    /// Selected fitness levels or an inclusive range.
    pub fitness_levels: Option<ScaleSelection>,
    /// Selected smoking habits or an inclusive range.
    pub smoking: Option<ScaleSelection>,
    /// Selected drinking habits or an inclusive range.
    pub drinking: Option<ScaleSelection>,
    /// Selected tattoo visibilities or an inclusive range.
    pub tattoos: Option<ScaleSelection>,
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
    /// Least `education_level`, an `EducationLevel` code; a profile that leaves it unset does not match.
    pub min_education: Option<u32>,
    /// Least `height` in centimetres, inclusive; any height bound leaves out a profile without one.
    pub min_height: Option<u32>,
    /// Greatest `height` in centimetres, inclusive.
    pub max_height: Option<u32>,
    /// Only profiles with at least one photo.
    pub with_photo: bool,
    /// Only profiles active (a heartbeat or publish) on the UTC day of this Unix second or later.
    pub active_since: Option<u64>,
    /// The searcher's gender code and age in years: only profiles that seek both.
    pub seeks: Option<(u32, u32)>,
}

/// Why [`Filter::conditions`] refused a filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterError {
    /// More than [`MAX_IN_VALUES`] values for one field.
    TooManyValues,
    /// Places or income without a country.
    NeedsCountry,
    /// A range whose lower bound is above its upper one.
    EmptyRange,
    /// Zero or a value that is not on the categorical scale.
    InvalidScale,
    /// More than 32 conditions in one node query.
    TooManyConditions,
}

impl Filter {
    /// The conditions for a query and for [`check`] at an explicit UTC date:
    /// at most 32, one per field.
    pub fn conditions(&self, today: CivilDate) -> Result<Vec<Condition>, FilterError> {
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
            let lower = max.checked_add(1).map_or(0, |age| today.age_cutoff(age));
            let upper = today.age_cutoff(min);
            if lower >= upper {
                return Err(FilterError::EmptyRange);
            }
            out.push(ints(BIRTH_AGE, Op::Range, vec![lower.into(), upper.into()]));
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
            if EducationLevel::from_code(min).is_none() {
                return Err(FilterError::EmptyRange);
            }
            out.push(ints(
                EducationLevel::FIELD,
                Op::Range,
                vec![min.into(), EducationLevel::MAX.into()],
            ));
        }
        if self.min_height.is_some() || self.max_height.is_some() {
            let min = self.min_height.unwrap_or(MIN_HEIGHT_CM);
            let max = self.max_height.unwrap_or(MAX_HEIGHT_CM);
            if min > max {
                return Err(FilterError::EmptyRange);
            }
            out.push(ints(HEIGHT_FIELD, Op::Range, vec![min.into(), max.into()]));
        }
        if self.with_photo {
            out.push(ints(21, Op::Range, vec![1, i64::MAX]));
        }
        if let Some(since) = self.active_since {
            out.push(ints(
                ACTIVE_DAY,
                Op::Range,
                vec![wide(since / 86_400), i64::MAX],
            ));
        }
        for (field, v) in [(8, self.has_kids), (9, self.wants_more_kids)] {
            if let Some(v) = v {
                out.push(ints(field, Op::Eq, vec![v.into()]));
            }
        }
        if self.places.len() > MAX_IN_VALUES {
            return Err(FilterError::TooManyValues);
        }
        if !self.places.is_empty() {
            out.push(Condition {
                field: 20,
                op: Op::In.into(),
                ints: Vec::new(),
                strings: self.places.clone(),
            });
        }
        for (field, values) in [
            (Gender::FIELD, &self.genders),
            (Orientation::FIELD, &self.orientations),
            (RelationshipGoal::FIELD, &self.relationship_goals),
            (RolePreference::FIELD, &self.role_preferences),
            (EmploymentStatus::FIELD, &self.employment_statuses),
            (Zodiac::FIELD, &self.zodiacs),
            (MaritalStatus::FIELD, &self.marital_statuses),
            (Pets::FIELD, &self.pets),
        ] {
            if values.len() > MAX_IN_VALUES {
                return Err(FilterError::TooManyValues);
            }
            let known = scale(field).ok_or(FilterError::InvalidScale)?;
            if values.iter().any(|&v| v == 0 || !known.knows(v)) {
                return Err(FilterError::InvalidScale);
            }
            if !values.is_empty() {
                out.push(ints(
                    field,
                    Op::In,
                    values.iter().copied().map(i64::from).collect(),
                ));
            }
        }
        for (field, selection) in [
            (CareerAmbition::FIELD, &self.career_ambitions),
            (FinancialPhilosophy::FIELD, &self.financial_philosophies),
            (FitnessLevel::FIELD, &self.fitness_levels),
            (Smoking::FIELD, &self.smoking),
            (Drinking::FIELD, &self.drinking),
            (Tattoos::FIELD, &self.tattoos),
        ] {
            let Some(selection) = selection else { continue };
            let known = scale(field).ok_or(FilterError::InvalidScale)?;
            let (op, values) = match selection {
                ScaleSelection::Any(values) => {
                    if values.is_empty() || values.len() > MAX_IN_VALUES {
                        return Err(FilterError::TooManyValues);
                    }
                    (Op::In, values.as_slice().to_vec())
                }
                ScaleSelection::Range(min, max) => {
                    if min > max {
                        return Err(FilterError::EmptyRange);
                    }
                    (Op::Range, vec![*min, *max])
                }
            };
            if values.iter().any(|&v| v == 0 || !known.knows(v)) {
                return Err(FilterError::InvalidScale);
            }
            out.push(ints(field, op, values.into_iter().map(i64::from).collect()));
        }
        if let Some((gender, age)) = self.seeks {
            let known = scale(Gender::FIELD).ok_or(FilterError::InvalidScale)?;
            if gender == 0
                || !known.knows(gender)
                || !(MIN_SEEKING_AGE..=MAX_SEEKING_AGE).contains(&age)
            {
                return Err(FilterError::InvalidScale);
            }
            out.push(ints(SEEKING_GENDERS, Op::Eq, vec![gender.into()]));
            out.push(ints(SEEKING_AGE, Op::Eq, vec![age.into()]));
        }
        if out.len() > 32 {
            return Err(FilterError::TooManyConditions);
        }
        Ok(out)
    }
}

mod seeking;
pub use seeking::mutual_conditions;
use seeking::seeking_age_matches;

#[cfg(test)]
mod seeking_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;

    #[test]
    fn a_filter_on_every_field_stays_within_the_node_limit() {
        let first = |field| vec![scale(field).unwrap().options[0].code];
        let any = |field| Some(ScaleSelection::Any(first(field)));
        let filter = Filter {
            age: Some((18, 35)),
            genders: first(Gender::FIELD),
            orientations: first(Orientation::FIELD),
            relationship_goals: first(RelationshipGoal::FIELD),
            role_preferences: first(RolePreference::FIELD),
            employment_statuses: first(EmploymentStatus::FIELD),
            zodiacs: first(Zodiac::FIELD),
            marital_statuses: first(MaritalStatus::FIELD),
            pets: first(Pets::FIELD),
            career_ambitions: any(CareerAmbition::FIELD),
            financial_philosophies: any(FinancialPhilosophy::FIELD),
            fitness_levels: any(FitnessLevel::FIELD),
            smoking: any(Smoking::FIELD),
            drinking: any(Drinking::FIELD),
            tattoos: any(Tattoos::FIELD),
            has_kids: Some(false),
            wants_more_kids: Some(true),
            country: "DE".into(),
            places: vec!["2950159".into()],
            income: Some((1, 2)),
            min_education: Some(EducationLevel::Bachelor.code()),
            min_height: Some(MIN_HEIGHT_CM),
            with_photo: true,
            active_since: Some(86_400),
            seeks: Some((Gender::Woman.code(), MIN_SEEKING_AGE)),
            ..Filter::default()
        };
        let today = CivilDate::new(2026, 10, 25).unwrap();
        assert_eq!(filter.conditions(today).unwrap().len(), 27);
    }

    #[test]
    fn habit_filters_keep_the_chosen_values_and_leave_out_an_empty_field() {
        let owner = Identity::generate();
        let filter = Filter {
            smoking: Some(ScaleSelection::Range(
                Smoking::No.code(),
                Smoking::Sometimes.code(),
            )),
            drinking: Some(ScaleSelection::Any(vec![Drinking::Yes.code()])),
            tattoos: Some(ScaleSelection::Any(vec![Tattoos::Hidden.code()])),
            marital_statuses: vec![MaritalStatus::Divorced.code()],
            pets: vec![Pets::Yes.code()],
            ..Filter::default()
        };
        let today = CivilDate::new(2026, 10, 25).unwrap();
        let conditions = filter.conditions(today).unwrap();
        assert_eq!(conditions.len(), 5);
        let fit = Profile {
            version: 1,
            smoking: Smoking::Sometimes.code(),
            drinking: Drinking::Yes.code(),
            tattoos: Tattoos::Hidden.code(),
            marital_status: MaritalStatus::Divorced.code(),
            pets: Pets::Yes.code(),
            ..Profile::default()
        };
        let kept = |p: &Profile| check(&conditions, 0, &[vec![p.sign(&owner)]]).len() == 1;
        assert!(kept(&fit));
        let misses: [fn(&mut Profile); 10] = [
            |p| p.smoking = Smoking::Yes.code(),
            |p| p.smoking = 0,
            |p| p.drinking = Drinking::No.code(),
            |p| p.drinking = 0,
            |p| p.tattoos = Tattoos::None.code(),
            |p| p.tattoos = 0,
            |p| p.marital_status = MaritalStatus::Married.code(),
            |p| p.marital_status = 0,
            |p| p.pets = Pets::No.code(),
            |p| p.pets = 0,
        ];
        for (i, miss) in misses.iter().enumerate() {
            let mut p = fit.clone();
            miss(&mut p);
            assert!(!kept(&p), "{i}");
        }
        let unknown_code = Filter {
            pets: vec![Pets::MAX + 1],
            ..Filter::default()
        };
        assert_eq!(
            unknown_code.conditions(today),
            Err(FilterError::InvalidScale)
        );
    }

    fn record(identity: &Identity, version: u64, age: u32, interests: &[&str]) -> SignedRecord {
        Profile {
            version,
            birth_year: Some(2026 - age),
            birth_month: Some(1),
            birth_day: Some(1),
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
            condition(BIRTH_AGE, Op::Range, &[19901025, 20011025], &[]),
            condition(19, Op::In, &[], &["chess", "ski"]),
            condition(4, Op::Eq, &[], &["DE"]),
            condition(99, Op::Eq, &[1], &[]),
        ];
        let found = check(&conditions, 0, &[honest, lying]);
        let years: Vec<_> = found.iter().map(|v| v.profile.birth_year).collect();
        assert_eq!(years, [Some(1993)]);
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
        let bad = condition(BIRTH_AGE, Op::Range, &[], &["30"]);
        assert!(check(&[bad], 0, &replies).is_empty());
        let reversed = condition(BIRTH_AGE, Op::Range, &[40, 20], &[]);
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
    fn a_code_above_the_maximum_is_not_stated_and_a_gap_code_falls_inside_its_range() {
        let p = Profile {
            career_ambition: CareerAmbition::MAX + 10,
            education_level: EducationLevel::MAX + 10,
            fitness_level: 15,
            ..Profile::default()
        };
        let hits = |field, op, ints: &[i64]| matches(&condition(field, op, ints, &[]), &p);
        let above = i64::from(CareerAmbition::MAX + 10);
        assert!(!hits(CareerAmbition::FIELD, Op::Eq, &[above]));
        assert!(!hits(CareerAmbition::FIELD, Op::Range, &[10, above]));
        assert!(!hits(27, Op::Eq, &[70]));
        assert!(hits(FitnessLevel::FIELD, Op::Range, &[10, 20]));
        assert!(!hits(FitnessLevel::FIELD, Op::Range, &[20, 40]));
    }

    #[test]
    fn a_range_over_an_unordered_scale_or_a_zero_code_matches_nothing() {
        let p = Profile {
            gender: Gender::MAX,
            ..Profile::default()
        };
        let hits = |op, ints: &[i64]| matches(&condition(Gender::FIELD, op, ints, &[]), &p);
        let code = i64::from(Gender::MAX);
        assert!(hits(Op::In, &[code]));
        assert!(!hits(Op::In, &[0, code]));
        assert!(!hits(Op::Range, &[1, code]));
    }

    #[test]
    fn every_field_and_op_is_checked() {
        let p = Profile {
            birth_year: Some(1996),
            country: "DE".into(),
            income_from: Some(10),
            income_to: Some(u64::MAX),
            has_kids: Some(true),
            wants_more_kids: Some(false),
            relationship_goal: RelationshipGoal::Marriage.code(),
            career_ambition: CareerAmbition::High.code(),
            orientation: Orientation::Bisexual.code(),
            role_preference: RolePreference::Equal.code(),
            zodiac: Zodiac::Aries.code(),
            financial_philosophy: FinancialPhilosophy::Saver.code(),
            employment_status: EmploymentStatus::Student.code(),
            fitness_level: FitnessLevel::Athlete.code(),
            interests: vec!["chess".into(), "go".into()],
            place: "f20".into(),
            gender: Gender::Woman.code(),
            education_level: EducationLevel::Specialist.code(),
            photos: vec![dyapp_profile::Photo::default()],
            ..Profile::default()
        };
        let hits = |field, op, ints: &[i64], strings: &[&str]| {
            matches(&condition(field, op, ints, strings), &p)
        };
        for (field, v) in [(6, 10), (7, i64::MAX), (8, 1), (9, 0), (21, 1), (27, 40)] {
            assert!(hits(field, Op::Eq, &[v], &[]), "{field}");
            assert!(!hits(field, Op::Eq, &[v - 1], &[]), "{field}");
        }
        for (field, v) in [
            (RelationshipGoal::FIELD, 2),
            (CareerAmbition::FIELD, 30),
            (Orientation::FIELD, 3),
            (RolePreference::FIELD, 2),
            (Zodiac::FIELD, 1),
            (FinancialPhilosophy::FIELD, 10),
            (EmploymentStatus::FIELD, 4),
            (FitnessLevel::FIELD, 40),
            (Gender::FIELD, 1),
        ] {
            assert!(hits(field, Op::Eq, &[v], &[]), "{field}");
            assert!(!hits(field, Op::Eq, &[0], &[]), "{field}");
            assert!(!hits(field, Op::Eq, &[v], &["wrong type"]), "{field}");
        }
        let fixed = [(4, "DE"), (19, "go"), (20, "f20")];
        for (field, v) in fixed {
            assert!(hits(field, Op::Eq, &[], &[v]), "{field}");
            assert!(!hits(field, Op::Eq, &[], &["x"]), "{field}");
        }
        let sixteen: Vec<i64> = (0..16).map(|i| i * 10).collect();
        let seventeen: Vec<i64> = (0..17).map(|i| i * 10).collect();
        assert!(hits(27, Op::Unspecified, &[], &[]));
        assert!(!hits(27, Op::Eq, &[40, 50], &[]));
        assert!(hits(27, Op::In, &[30, 40], &[]));
        assert!(!hits(27, Op::In, &[], &[]));
        assert!(hits(27, Op::In, &sixteen, &[]));
        assert!(!hits(27, Op::In, &seventeen, &[]));
        assert!(hits(27, Op::Range, &[30, 50], &[]));
        assert!(!hits(27, Op::Range, &[30, 40, 50], &[]));
        let mut sixteen = vec!["other"; 15];
        sixteen.push("DE");
        let mut seventeen = sixteen.clone();
        seventeen.push("other");
        assert!(hits(4, Op::Unspecified, &[], &[]));
        assert!(hits(4, Op::Range, &[], &[]));
        assert!(!hits(4, Op::Eq, &[], &["DE", "FR"]));
        assert!(hits(4, Op::In, &[], &["FR", "DE"]));
        assert!(!hits(4, Op::In, &[], &["FR"]));
        assert!(hits(4, Op::In, &[], &sixteen));
        assert!(!hits(4, Op::In, &[], &seventeen));
    }

    #[test]
    fn a_filter_keeps_the_matching_profile_and_drops_each_mismatch() {
        let a = Identity::generate();
        let today = CivilDate::new(2026, 10, 25).unwrap();
        let fit = Profile {
            version: 1,
            birth_year: Some(1996),
            birth_month: Some(5),
            birth_day: Some(1),
            gender: Gender::Woman.code(),
            orientation: Orientation::Heterosexual.code(),
            relationship_goal: RelationshipGoal::Marriage.code(),
            has_kids: Some(false),
            wants_more_kids: Some(true),
            country: "DE".into(),
            place: "2950159".into(),
            income_from: Some(40_000),
            income_to: Some(60_000),
            education_level: EducationLevel::Specialist.code(),
            photos: vec![dyapp_profile::Photo {
                full: vec![vec![1; 32]],
                thumbnail: vec![2; 32],
            }],
            ..Profile::default()
        };
        let strings = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let filter = Filter {
            age: Some((0, 35)),
            genders: vec![Gender::Woman.code(), Gender::NonBinary.code()],
            orientations: vec![Orientation::Heterosexual.code()],
            relationship_goals: vec![
                RelationshipGoal::Marriage.code(),
                RelationshipGoal::SeriousRelationship.code(),
            ],
            has_kids: Some(false),
            wants_more_kids: Some(true),
            country: "DE".into(),
            places: strings(&["2950159"]),
            income: Some((55_000, u64::MAX)),
            min_education: Some(EducationLevel::Bachelor.code()),
            with_photo: true,
            active_since: Some(86_400 * 3 - 1),
            ..Filter::default()
        };
        let conditions = filter.conditions(today).unwrap();
        assert_eq!(conditions.len(), 13);
        assert_eq!(conditions[6].ints, [2, i64::MAX]);
        assert_eq!(conditions[0].field, BIRTH_AGE);
        assert_eq!(conditions[0].ints, [19901025, 20081025]);
        let kept = |p: &Profile| check(&conditions, 0, &[vec![p.sign(&a)]]).len() == 1;
        assert!(kept(&fit));
        let misses: [fn(&mut Profile); 12] = [
            |p| p.birth_year = Some(1990),
            |p| p.gender = Gender::Man.code(),
            |p| p.orientation = Orientation::GayLesbian.code(),
            |p| p.relationship_goal = RelationshipGoal::Casual.code(),
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
        assert_eq!(Filter::default().conditions(today).unwrap(), []);
    }

    #[test]
    fn current_age_uses_birth_bounds_and_leap_anniversary() {
        let owner = Identity::generate();
        let filter = Filter {
            age: Some((21, 21)),
            ..Filter::default()
        };
        let leap = Profile {
            version: 1,
            birth_year: Some(2004),
            birth_month: Some(2),
            birth_day: Some(29),
            ..Profile::default()
        };
        let before = filter
            .conditions(CivilDate::new(2025, 2, 28).unwrap())
            .unwrap();
        assert_eq!(before[0].ints, [20030228, 20040228]);
        assert!(check(&before, 0, &[vec![leap.sign(&owner)]]).is_empty());
        let after = filter
            .conditions(CivilDate::new(2025, 3, 1).unwrap())
            .unwrap();
        assert_eq!(after[0].ints, [20030301, 20040301]);
        assert_eq!(check(&after, 0, &[vec![leap.sign(&owner)]]).len(), 1);

        for profile in [
            Profile {
                birth_month: None,
                ..leap.clone()
            },
            Profile {
                birth_year: None,
                ..leap.clone()
            },
            Profile {
                birth_month: None,
                birth_day: None,
                ..leap.clone()
            },
            Profile {
                birth_year: None,
                birth_month: None,
                birth_day: None,
                ..leap.clone()
            },
        ] {
            // A broad partial date or no year cannot prove age 21.
            assert!(check(&after, 0, &[vec![profile.sign(&owner)]]).is_empty());
        }
        assert!(!passes(
            &[condition(BIRTH_AGE, Op::Eq, &[21], &[])],
            0,
            &leap
        ));
        assert!(!passes(
            &[condition(BIRTH_AGE, Op::Range, &[1, 1], &[])],
            0,
            &leap
        ));
    }

    #[test]
    fn a_height_filter_takes_either_bound_and_rechecks_inclusively() {
        let today = CivilDate::new(2026, 10, 25).unwrap();
        let keeps = |min_height, max_height, height| {
            let filter = Filter {
                min_height,
                max_height,
                ..Filter::default()
            };
            let profile = Profile {
                height,
                ..Profile::default()
            };
            let conditions = filter.conditions(today).unwrap();
            conditions.iter().all(|c| matches(c, &profile))
        };
        assert!(keeps(Some(170), None, Some(170)));
        assert!(!keeps(Some(170), None, Some(169)));
        assert!(keeps(None, Some(170), Some(170)));
        assert!(!keeps(None, Some(170), Some(171)));
        assert!(keeps(Some(170), Some(170), Some(170)));
        assert!(!keeps(Some(170), None, None));
        assert!(!keeps(None, Some(170), None));
        assert!(keeps(None, None, None));
    }

    #[test]
    fn a_filter_refuses_what_a_node_would_not_take() {
        let today = CivilDate::new(2026, 10, 25).unwrap();
        let open = |filter: Filter| filter.conditions(today).unwrap_err();
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
        for (min_height, max_height) in
            [(Some(181), Some(180)), (Some(273), None), (None, Some(49))]
        {
            let height = Filter {
                min_height,
                max_height,
                ..Filter::default()
            };
            assert_eq!(open(height), FilterError::EmptyRange);
        }
        for min in [0, 15, 70] {
            let level = Filter {
                min_education: Some(min),
                ..Filter::default()
            };
            assert_eq!(open(level), FilterError::EmptyRange);
        }
        let many = Filter {
            country: "DE".into(),
            places: (0..17).map(|i| i.to_string()).collect(),
            ..Filter::default()
        };
        assert_eq!(open(many), FilterError::TooManyValues);
        let sixteen = Filter {
            places: (0..16).map(|i| i.to_string()).collect(),
            age: Some((18, 18)),
            country: "DE".into(),
            income: Some((1, 1)),
            ..Filter::default()
        };
        assert_eq!(sixteen.conditions(today).unwrap().len(), 5);
        for value in [0, Gender::MAX + 1] {
            assert_eq!(
                open(Filter {
                    genders: vec![value],
                    ..Filter::default()
                }),
                FilterError::InvalidScale
            );
        }
        assert_eq!(
            open(Filter {
                career_ambitions: Some(ScaleSelection::Range(10, 15)),
                ..Filter::default()
            }),
            FilterError::InvalidScale
        );
        assert_eq!(
            open(Filter {
                career_ambitions: Some(ScaleSelection::Range(30, 10)),
                ..Filter::default()
            }),
            FilterError::EmptyRange
        );
    }
}
