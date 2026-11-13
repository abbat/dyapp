//! Transport-only names; exhaustive conversion follows the shared comparison vocabulary.
use dyapp_client::publish::{AdoptionField, AdoptionSource};

macro_rules! fields {
    ($($name:ident),+ $(,)?) => {
        /// Editable field groups offered by the core, never device authorization or proof metadata.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
        pub enum LinkReviewField { $(#[doc = "Shared core editable field group."] $name),+ }

        impl From<AdoptionField> for LinkReviewField {
            fn from(field: AdoptionField) -> Self {
                match field { $(AdoptionField::$name => Self::$name),+ }
            }
        }
        impl From<LinkReviewField> for AdoptionField {
            fn from(field: LinkReviewField) -> Self {
                match field { $(LinkReviewField::$name => Self::$name),+ }
            }
        }
    };
}
fields! {
    Name, Location, Income, HasKids, WantsMoreKids, Interests, Photos, Education,
    Profession, Company, Industry, CareerStage, WorkLifeBalance, Birth, Gender,
    Orientation, RelationshipGoal, RolePreference, CareerAmbition, FinancialPhilosophy,
    EmploymentStatus, FitnessLevel, Zodiac, Height, About, Smoking, Drinking, Tattoos,
    MaritalStatus, Pets, Seeking,
}

/// Which of the two immutable values the owner explicitly selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LinkReviewSource {
    /// Preserve the local value.
    Local,
    /// Use the verified remote value.
    Remote,
}
impl From<LinkReviewSource> for AdoptionSource {
    fn from(source: LinkReviewSource) -> Self {
        match source {
            LinkReviewSource::Local => Self::Local,
            LinkReviewSource::Remote => Self::Remote,
        }
    }
}

/// One explicit decision; the core rejects missing, duplicate or unoffered fields.
#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct LinkReviewChoice {
    /// Offered field group.
    pub field: LinkReviewField,
    /// The selected immutable source.
    pub source: LinkReviewSource,
}
impl From<LinkReviewChoice> for (AdoptionField, AdoptionSource) {
    fn from(choice: LinkReviewChoice) -> Self {
        (choice.field.into(), choice.source.into())
    }
}
