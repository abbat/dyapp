//! The editable comparison vocabulary deliberately excludes version, proof and device authorization.
use dyapp_profile::Profile;

macro_rules! fields {
    ($($variant:ident($doc:literal) => [$($field:ident),+]),+ $(,)?) => {
        /// An indivisible profile field or validity-coupled group offered for owner review.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum AdoptionField { $(#[doc = $doc] $variant),+ }

        impl AdoptionField {
            pub(super) const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub(super) fn differs(self, local: &Profile, remote: &Profile) -> bool {
                match self { $(Self::$variant => ($(&local.$field,)+) != ($(&remote.$field,)+)),+ }
            }

            pub(super) fn copy(self, source: &Profile, target: &mut Profile) {
                // New wire fields must be classified as editable or protected before this compiles.
                let Profile { version: _, deleted: _, pow_nonce: _, pow_bits: _, devices: _,
                    device_owners: _, previous: _, $($($field: _),+),+ } = source;
                match self { $(Self::$variant => { $(target.$field.clone_from(&source.$field);)+ }),+ }
            }
        }
    };
}

fields! {
    Name("Public name.") => [name],
    Location("Country and selected place together.") => [country, place],
    Income("Both income bounds together.") => [income_from, income_to],
    HasKids("Whether the owner has children.") => [has_kids],
    WantsMoreKids("Whether the owner wants more children.") => [wants_more_kids],
    Interests("The complete interest list.") => [interests],
    Photos("The ordered photo references, including thumbnails and originals.") => [photos],
    Education("Education level.") => [education_level],
    Profession("Profession.") => [profession],
    Company("Employer.") => [company],
    Industry("Industry.") => [industry],
    CareerStage("Career stage.") => [career_stage],
    WorkLifeBalance("Work-life balance.") => [work_life_balance],
    Birth("All parts of the partial birth date together.") => [birth_year, birth_month, birth_day],
    Gender("Gender.") => [gender],
    Orientation("Orientation.") => [orientation],
    RelationshipGoal("Relationship goal.") => [relationship_goal],
    RolePreference("Role preference.") => [role_preference],
    CareerAmbition("Career ambition.") => [career_ambition],
    FinancialPhilosophy("Financial philosophy.") => [financial_philosophy],
    EmploymentStatus("Employment status.") => [employment_status],
    FitnessLevel("Fitness level.") => [fitness_level],
    Zodiac("Zodiac sign.") => [zodiac],
    Height("Optional height.") => [height],
    About("About text.") => [about],
    Smoking("Smoking choice.") => [smoking],
    Drinking("Drinking choice.") => [drinking],
    Tattoos("Tattoo choice.") => [tattoos],
    MaritalStatus("Marital status.") => [marital_status],
    Pets("Pet choice.") => [pets],
    Seeking("Seeking genders, age bounds and not-looking flag together.") =>
        [seeking_genders, seeking_age_from, seeking_age_to, not_looking],
}
