//! Wire values for public categorical profile fields. Zero means not published.
//! Ordered scales leave gaps between codes so a later value can sit between two others.
//! Variants are declared in ascending code order.

macro_rules! scale {
    ($name:ident, $field:expr, $ordered:expr, [$($variant:ident = $code:expr),+ $(,)?]) => {
        #[doc = concat!("Closed wire scale for ", stringify!($name), ".")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u32)]
        pub enum $name { $(#[doc = concat!("Wire value ", stringify!($variant), ".")] $variant = $code),+ }

        impl $name {
            /// Protobuf field number used by search.
            pub const FIELD: u32 = $field;
            /// Highest valid wire value.
            pub const MAX: u32 = {
                let codes = [$($code),+];
                codes[codes.len() - 1]
            };
            /// Whether range queries have a meaningful order.
            pub const ORDERED: bool = $ordered;
            /// Every published value in wire order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            /// Wire values and stable localization keys.
            pub const OPTIONS: &'static [ScaleOption] = &[
                $(ScaleOption { code: $code, key: stringify!($variant) }),+
            ];

            /// Numeric value in the signed profile.
            pub const fn code(self) -> u32 { self as u32 }

            /// Stable localization key; clients translate this key for display.
            pub const fn key(self) -> &'static str {
                match self { $(Self::$variant => stringify!($variant)),+ }
            }

            /// Decodes a published value; zero and unknown values have no variant.
            pub fn from_code(code: u32) -> Option<Self> {
                Self::ALL.iter().copied().find(|value| value.code() == code)
            }
        }
    };
}

// Self-declared gender.
scale!(
    Gender,
    36,
    false,
    [Woman = 1, Man = 2, NonBinary = 3, Other = 4]
);
// Self-declared orientation.
scale!(
    Orientation,
    37,
    false,
    [
        Heterosexual = 1,
        GayLesbian = 2,
        Bisexual = 3,
        Asexual = 4,
        Other = 5
    ]
);
// Desired relationship.
scale!(
    RelationshipGoal,
    38,
    false,
    [
        SeriousRelationship = 1,
        Marriage = 2,
        Casual = 3,
        Friendship = 4,
        NotSureYet = 5
    ]
);
// Preferred role in the family.
scale!(
    RolePreference,
    39,
    false,
    [Breadwinner = 1, Equal = 2, Homemaker = 3]
);
// Ordered career ambition.
scale!(
    CareerAmbition,
    40,
    true,
    [Low = 10, Moderate = 20, High = 30]
);
// Ordered financial philosophy.
scale!(
    FinancialPhilosophy,
    41,
    true,
    [Saver = 10, Balanced = 20, Spender = 30]
);
// Employment status.
scale!(
    EmploymentStatus,
    42,
    false,
    [
        FullTime = 1,
        PartTime = 2,
        SelfEmployedFreelance = 3,
        Student = 4,
        NotWorking = 5,
        Retired = 6
    ]
);
// Ordered fitness level.
scale!(
    FitnessLevel,
    43,
    true,
    [Sedentary = 10, Moderate = 20, Active = 30, Athlete = 40]
);
// Self-declared zodiac sign, in calendar order.
scale!(
    Zodiac,
    44,
    false,
    [
        Aries = 1,
        Taurus = 2,
        Gemini = 3,
        Cancer = 4,
        Leo = 5,
        Virgo = 6,
        Libra = 7,
        Scorpio = 8,
        Sagittarius = 9,
        Capricorn = 10,
        Aquarius = 11,
        Pisces = 12
    ]
);

// Highest education. Not in `SCALES`: no client has a picker for it yet.
scale!(
    EducationLevel,
    27,
    true,
    [
        Secondary = 10,
        VocationalOrCollege = 20,
        Bachelor = 30,
        Specialist = 40,
        Master = 50,
        Doctorate = 60
    ]
);

/// A code as search sees it: zero and a code above `maximum`, from a newer client, are not stated.
/// A code in a gap below `maximum` stays, so a value added later falls inside its ranges.
pub const fn stated(code: u32, maximum: u32) -> Option<u32> {
    if code == 0 || code > maximum {
        None
    } else {
        Some(code)
    }
}

/// One published value and its stable localization key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaleOption {
    /// Numeric value in the signed profile.
    pub code: u32,
    /// Key translated by each client's string catalogue.
    pub key: &'static str,
}

/// A searchable closed scale, for clients and search nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scale {
    /// Protobuf field number.
    pub field: u32,
    /// Stable field name.
    pub key: &'static str,
    /// Highest valid value.
    pub max: u32,
    /// Whether range queries are meaningful.
    pub ordered: bool,
    /// Wire values in display order.
    pub options: &'static [ScaleOption],
}

impl Scale {
    /// Whether a client may publish `code`: zero or exactly one of this scale's values.
    pub fn knows(&self, code: u32) -> bool {
        code == 0 || self.options.iter().any(|option| option.code == code)
    }
}

macro_rules! entry {
    ($name:ident, $key:literal) => {
        Scale {
            field: $name::FIELD,
            key: $key,
            max: $name::MAX,
            ordered: $name::ORDERED,
            options: $name::OPTIONS,
        }
    };
}

/// All categorical scales in the signed profile.
pub const SCALES: &[Scale] = &[
    entry!(Gender, "gender"),
    entry!(Orientation, "orientation"),
    entry!(RelationshipGoal, "relationship_goal"),
    entry!(RolePreference, "role_preference"),
    entry!(CareerAmbition, "career_ambition"),
    entry!(FinancialPhilosophy, "financial_philosophy"),
    entry!(EmploymentStatus, "employment_status"),
    entry!(FitnessLevel, "fitness_level"),
    entry!(Zodiac, "zodiac"),
];

/// Returns the closed scale for a searchable protobuf field.
pub fn scale(field: u32) -> Option<Scale> {
    SCALES.iter().copied().find(|scale| scale.field == field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_ascend_and_ordered_scales_leave_gaps() {
        let education = entry!(EducationLevel, "education_level");
        for scale in SCALES.iter().chain([&education]) {
            let codes: Vec<u32> = scale.options.iter().map(|option| option.code).collect();
            assert!(
                codes.windows(2).all(|pair| pair[0] < pair[1]),
                "{}",
                scale.key
            );
            assert_eq!(codes.last(), Some(&scale.max), "{}", scale.key);
            if scale.ordered {
                assert!(codes.iter().all(|code| code % 10 == 0), "{}", scale.key);
            }
        }
    }

    #[test]
    fn a_client_knows_only_exact_codes_and_search_drops_only_those_above_the_maximum() {
        let career = scale(CareerAmbition::FIELD).unwrap();
        assert!(career.knows(0) && career.knows(20));
        assert!(!career.knows(15) && !career.knows(CareerAmbition::MAX + 10));
        assert_eq!(stated(0, CareerAmbition::MAX), None);
        assert_eq!(stated(15, CareerAmbition::MAX), Some(15));
        assert_eq!(stated(30, CareerAmbition::MAX), Some(30));
        assert_eq!(stated(31, CareerAmbition::MAX), None);
    }
}
