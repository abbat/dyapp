//! Pasted link-code validation shared with the existing desktop contract.

use super::Code;

/// Existing presentation distinction; wire parsing remains unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CodeInputError {
    /// Malformed code, including an invalid zero expiry.
    #[error("bad")]
    Bad,
    /// A well-formed code whose expiry has arrived.
    #[error("expired")]
    Expired,
}

/// Validate owner input before asking for explicit full-control approval.
///
/// # Errors
/// Returns the existing malformed/expired distinction, preserving desktop pasted-code trimming.
pub fn parse_input(text: &str, now: u64) -> Result<Code, CodeInputError> {
    let text = text.trim();
    Code::parse(text, 0).map_err(|_| CodeInputError::Bad)?;
    Code::parse(text, now).map_err(|_| CodeInputError::Expired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_and_expired_inputs_keep_the_existing_desktop_distinction() {
        let code = Code {
            public: [1; 32],
            mailbox: [2; 32],
            expires_at: 400,
        };
        let text = code.text();
        assert_eq!(parse_input(&text, 399), Ok(code));
        assert_eq!(parse_input(&text, 400), Err(CodeInputError::Expired));
        assert_eq!(parse_input("not a link", 400), Err(CodeInputError::Bad));
        let zero = Code {
            expires_at: 0,
            ..code
        }
        .text();
        assert_eq!(parse_input(&zero, 0), Err(CodeInputError::Bad));
        assert_eq!(parse_input(&format!(" \n\u{0085}{text}\t "), 399), Ok(code));
        assert_eq!(
            parse_input(&format!("\u{feff}{text}"), 399),
            Err(CodeInputError::Bad)
        );
    }
}
