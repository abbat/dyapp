//! Native conversion of the shared raw employer-filter parser.

use crate::SearchEmployer;
use dyapp_client::search::Employer;
use dyapp_client::search_input::{employer_filter, EmployerMode};

/// The employer choice before interpreting the form's raw company text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SearchEmployerMode {
    /// Ignore company text.
    Any,
    /// One company, including any literal semicolons.
    Same,
    /// Semicolon-separated companies to exclude locally.
    Exclude,
}

/// Interpret employer form input in the shared core; no node receives this filter.
#[uniffi::export]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
pub fn search_employer_filter(mode: SearchEmployerMode, text: String) -> SearchEmployer {
    let mode = match mode {
        SearchEmployerMode::Any => EmployerMode::Any,
        SearchEmployerMode::Same => EmployerMode::Same,
        SearchEmployerMode::Exclude => EmployerMode::Exclude,
    };
    match employer_filter(mode, &text) {
        Employer::Any => SearchEmployer::Any,
        Employer::Same(company) => SearchEmployer::Same { company },
        Employer::Exclude(companies) => SearchEmployer::Exclude { companies },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_modes_preserve_company_payloads_and_separator_rules() {
        assert_eq!(
            search_employer_filter(SearchEmployerMode::Same, " Acme; Inc ".into()),
            SearchEmployer::Same {
                company: "Acme; Inc".into()
            }
        );
        assert_eq!(
            search_employer_filter(SearchEmployerMode::Exclude, " Acme; ;Globex;Acme; ".into()),
            SearchEmployer::Exclude {
                companies: vec!["Acme".into(), "Globex".into(), "Acme".into()]
            }
        );
    }

    #[test]
    fn blank_and_any_inputs_have_one_shared_meaning() {
        for mode in [
            SearchEmployerMode::Any,
            SearchEmployerMode::Same,
            SearchEmployerMode::Exclude,
        ] {
            assert_eq!(
                search_employer_filter(mode, "\u{feff}\u{a0}".into()),
                SearchEmployer::Any
            );
        }
        assert_eq!(
            search_employer_filter(SearchEmployerMode::Any, "Acme".into()),
            SearchEmployer::Any
        );
    }

    #[test]
    fn edge_payloads_are_converted_without_platform_reinterpretation() {
        assert_eq!(
            search_employer_filter(SearchEmployerMode::Same, "\u{85}".into()),
            SearchEmployer::Same {
                company: "\u{85}".into()
            }
        );
        assert_eq!(
            search_employer_filter(SearchEmployerMode::Exclude, "; ;".into()),
            SearchEmployer::Exclude {
                companies: Vec::new()
            }
        );
    }
}
