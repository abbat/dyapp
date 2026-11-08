//! The text of a shared profile: an ordinary chat message whose whole body is the link, so an old
//! client shows plain text. It carries only a Peer ID: opening it goes through the verified profile
//! fetch, so a forged link can only point at a real signed profile.

use crate::safety::parse_peer_id;

const PREFIX: &str = "dyapp-profile:1:";

/// The link to a canonical lowercase hexadecimal Peer ID.
#[must_use]
pub fn profile_link(peer_id: &str) -> Option<String> {
    parse_peer_id(peer_id)?;
    Some(format!("{PREFIX}{peer_id}"))
}

/// The Peer ID of a message body that is exactly one link; a link inside other text is just text.
#[must_use]
pub fn parse_profile_link(text: &str) -> Option<&str> {
    let peer_id = text.strip_prefix(PREFIX)?;
    parse_peer_id(peer_id)?;
    Some(peer_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEER: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn link_round_trips_and_has_a_stable_format() {
        let text = profile_link(PEER).unwrap();
        assert_eq!(text, format!("dyapp-profile:1:{PEER}"));
        assert_eq!(parse_profile_link(&text), Some(PEER));
    }

    #[test]
    fn only_a_canonical_peer_id_makes_a_link() {
        for bad in [
            "",
            &PEER[..63],
            PEER.to_uppercase().as_str(),
            format!("{PEER}0").as_str(),
        ] {
            assert_eq!(profile_link(bad), None, "{bad}");
        }
    }

    #[test]
    fn anything_but_exactly_one_link_stays_text() {
        let upper = format!("dyapp-profile:1:{}", PEER.to_uppercase());
        let wide = format!("dyapp-profile:1:{}é", &PEER[..62]);
        for bad in [
            String::new(),
            PEER.to_owned(),
            format!("dyapp-profile:2:{PEER}"),
            format!("DYAPP-PROFILE:1:{PEER}"),
            format!(" dyapp-profile:1:{PEER}"),
            format!("dyapp-profile:1:{PEER}\n"),
            format!("dyapp-profile:1:{PEER}:1"),
            format!("see dyapp-profile:1:{PEER}"),
            upper,
            wide,
        ] {
            assert_eq!(parse_profile_link(&bad), None, "{bad}");
        }
    }
}
