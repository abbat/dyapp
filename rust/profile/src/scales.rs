//! Wire values for public categorical profile fields. Zero means not published.

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
            pub const MAX: u32 = [$(Self::$variant as u32),+].len() as u32;
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
scale!(CareerAmbition, 40, true, [Low = 1, Moderate = 2, High = 3]);
// Ordered financial philosophy.
scale!(
    FinancialPhilosophy,
    41,
    true,
    [Saver = 1, Balanced = 2, Spender = 3]
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
    [Sedentary = 1, Moderate = 2, Active = 3, Athlete = 4]
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
