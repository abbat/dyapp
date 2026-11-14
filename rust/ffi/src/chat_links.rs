//! Thin native conversion of shared web-destination presentation.

/// A web destination; display ASCII even when an additional Unicode spelling is present.
#[derive(Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatLinkHost {
    /// Canonical destination host and optional nondefault port, without credentials.
    pub ascii: String,
    /// Additional Unicode IDN spelling, never authorization or a replacement for ASCII.
    pub unicode: Option<String>,
}

/// Find HTTP(S) destinations through the shared core without opening or fetching them.
#[uniffi::export]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts an owned string across the native boundary"
)]
pub fn chat_link_hosts(text: String) -> Vec<ChatLinkHost> {
    dyapp_client::chat_links::hosts(&text)
        .into_iter()
        .map(|host| ChatLinkHost {
            ascii: host.ascii,
            unicode: host.unicode,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_projection_keeps_actual_ascii_and_unicode_destinations() {
        assert_eq!(
            chat_link_hosts(
                "https://bücher.example:8443/ https://safe.example@evil.example".into()
            ),
            [
                ChatLinkHost {
                    ascii: "xn--bcher-kva.example:8443".into(),
                    unicode: Some("bücher.example:8443".into()),
                },
                ChatLinkHost {
                    ascii: "evil.example".into(),
                    unicode: None,
                },
            ]
        );
        assert_eq!(chat_link_hosts("javascript:alert(1)".into()), Vec::new());
    }
}
