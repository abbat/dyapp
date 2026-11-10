//! Presentation-independent interpretation of raw profile search ranges.

/// Raw age and height fields, before parsing or applying query conditions.
#[derive(Clone, Debug, Default)]
pub struct SearchRangeInputs {
    /// Inclusive minimum age.
    pub age_min: String,
    /// Inclusive maximum age.
    pub age_max: String,
    /// Inclusive minimum height in centimetres.
    pub height_min: String,
    /// Inclusive maximum height in centimetres.
    pub height_max: String,
}

/// Parsed ranges for the shared query validator; absent ranges add no condition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchRanges {
    /// Inclusive age bounds, with missing bounds defaulted to 18 and 150.
    pub age: Option<(u32, u32)>,
    /// Inclusive minimum height; absent means no lower constraint.
    pub min_height: Option<u32>,
    /// Inclusive maximum height; absent means no upper constraint.
    pub max_height: Option<u32>,
}

/// The field whose entered number cannot be represented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SearchInputError {
    /// An age bound is not a nonnegative whole number in the supported integer range.
    #[error("invalid search age")]
    Age,
    /// A height bound is not a nonnegative whole number in the supported integer range.
    #[error("invalid search height")]
    Height,
}

fn number(text: &str, error: SearchInputError) -> Result<Option<u32>, SearchInputError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse().map(Some).map_err(|_| error)
}

impl SearchRangeInputs {
    /// Interpret only offered fields; hidden malformed input is ignored.
    ///
    /// Blank age fields add no range; entering either uses 18/150 for a missing bound.
    /// Height bounds remain optional. Query validation subsequently checks range semantics.
    ///
    /// # Errors
    /// Returns the field-specific error for a nonnumeric or overflowing bound.
    pub fn parse(
        &self,
        age_offered: bool,
        height_offered: bool,
    ) -> Result<SearchRanges, SearchInputError> {
        let mut ranges = SearchRanges::default();
        if age_offered && (!self.age_min.trim().is_empty() || !self.age_max.trim().is_empty()) {
            ranges.age = Some((
                number(&self.age_min, SearchInputError::Age)?.unwrap_or(18),
                number(&self.age_max, SearchInputError::Age)?.unwrap_or(150),
            ));
        }
        if height_offered {
            ranges.min_height = number(&self.height_min, SearchInputError::Height)?;
            ranges.max_height = number(&self.height_max, SearchInputError::Height)?;
        }
        Ok(ranges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_and_hidden_ranges_stay_absent() {
        assert_eq!(
            SearchRangeInputs::default().parse(true, true),
            Ok(SearchRanges::default())
        );
        let inputs = SearchRangeInputs {
            age_min: "invalid".into(),
            height_max: "invalid".into(),
            ..SearchRangeInputs::default()
        };
        assert_eq!(inputs.parse(false, false), Ok(SearchRanges::default()));
    }

    #[test]
    fn missing_age_bounds_use_the_shared_defaults() {
        for (min, max, expected) in [("", " 30 ", (18, 30)), (" 21 ", "", (21, 150))] {
            let inputs = SearchRangeInputs {
                age_min: min.into(),
                age_max: max.into(),
                height_min: "50".into(),
                height_max: "272".into(),
            };
            let parsed = inputs.parse(true, true).expect("valid inputs");
            assert_eq!(parsed.age, Some(expected));
            assert_eq!(parsed.min_height, Some(50));
            assert_eq!(parsed.max_height, Some(272));
        }
    }

    #[test]
    fn invalid_numbers_report_the_field() {
        for value in ["invalid", "-1", "4294967296", "1.5"] {
            let mut inputs = SearchRangeInputs {
                age_min: value.into(),
                ..SearchRangeInputs::default()
            };
            assert_eq!(inputs.parse(true, false), Err(SearchInputError::Age));
            inputs.height_max = value.into();
            assert_eq!(inputs.parse(false, true), Err(SearchInputError::Height));
        }
    }
}
