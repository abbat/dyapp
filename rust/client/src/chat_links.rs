//! Display-only web destinations shared by clients; never fetch or open a link here.

use dyapp_profile::text_input::is_whitespace;
use idna::uts46::{AsciiDenyList, Hyphens, Uts46};
use std::collections::HashSet;
use url::{Position, Url};

/// The actual HTTP(S) destination, excluding credentials and preserving a nondefault port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkHost {
    /// Canonical ASCII host (Punycode for IDNs), always required in destination display.
    pub ascii: String,
    /// An additional Unicode spelling for IDNs; never a replacement for the ASCII host.
    pub unicode: Option<String>,
}

fn delimiter(ch: char) -> bool {
    is_whitespace(ch) || matches!(ch, '<' | '>' | '"' | '\'')
}

fn destination(text: &str) -> Option<LinkHost> {
    let url = Url::parse(text).ok()?;
    if !matches!(url.scheme(), "http" | "https") || !url.has_host() {
        return None;
    }
    let ascii = url[Position::BeforeHost..Position::AfterPort].to_owned();
    let unicode = url.domain().and_then(|domain| {
        let (decoded, valid) =
            Uts46::new().to_unicode(domain.as_bytes(), AsciiDenyList::URL, Hyphens::Allow);
        (valid.is_ok() && !decoded.is_ascii()).then(|| {
            format!(
                "{decoded}{}",
                &url[Position::AfterHost..Position::AfterPort]
            )
        })
    });
    Some(LinkHost { ascii, unicode })
}

/// Return distinct destinations in first-occurrence order using desktop token boundaries.
///
/// Only explicit HTTP(S) URLs count. Trailing `),.!?` is prose punctuation, as in the
/// desktop scanner. Userinfo never supplies the host; malformed URLs remain plain text.
/// This projection neither resolves hosts nor makes network requests or opening decisions.
#[must_use]
pub fn hosts(text: &str) -> Vec<LinkHost> {
    let mut remaining = text;
    let mut seen = HashSet::new();
    let mut hosts = Vec::new();
    while let Some(start) = remaining.find(['h', 'H']) {
        remaining = &remaining[start..];
        let web = remaining
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
            || remaining
                .get(..8)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"));
        if !web {
            remaining = &remaining[1..];
            continue;
        }
        let end = remaining.find(delimiter).unwrap_or(remaining.len());
        let candidate = remaining[..end].trim_end_matches([')', ',', '.', '!', '?']);
        remaining = &remaining[end..];
        if let Some(host) = destination(candidate) {
            if seen.insert(host.ascii.clone()) {
                hosts.push(host);
            }
        }
    }
    hosts
}

#[cfg(test)]
mod tests;
