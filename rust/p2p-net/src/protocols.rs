//! Validated protocol namespaces; current swarm builders still use the public constants.

use crate::{
    DIGEST_PROTOCOL, IDENTIFY_PROTOCOL, INVENTORY_PROTOCOL, KAD_PROTOCOL, MAILBOX_PROTOCOL,
    MAILBOX_PUSH_PROTOCOL, MEDIA_PROTOCOL, NODE_PROTOCOL, PROFILE_PROTOCOL, SEARCH_KAD_PROTOCOL,
    SEARCH_PROTOCOL,
};
use libp2p::{gossipsub, StreamProtocol};

/// IDs of one network. Named networks use a domain-separated hash rather than the raw name.
#[derive(Clone, Debug)]
pub struct Protocols {
    pub version: String,
    pub kad: StreamProtocol,
    pub search_kad: StreamProtocol,
    pub node: StreamProtocol,
    pub profile: StreamProtocol,
    pub mailbox: StreamProtocol,
    pub mailbox_push: StreamProtocol,
    pub media: StreamProtocol,
    pub search: StreamProtocol,
    pub inventory: StreamProtocol,
    /// Reserved for the future digest behaviour; not included in `all()` yet.
    pub digest: StreamProtocol,
    /// None preserves libp2p's public gossipsub prefix.
    pub gossip_prefix: Option<String>,
}

impl Protocols {
    /// The existing public network, with unchanged protocol IDs and gossip defaults.
    pub fn public() -> Self {
        Self {
            version: IDENTIFY_PROTOCOL.to_owned(),
            kad: KAD_PROTOCOL,
            search_kad: SEARCH_KAD_PROTOCOL,
            node: NODE_PROTOCOL,
            profile: PROFILE_PROTOCOL,
            mailbox: MAILBOX_PROTOCOL,
            mailbox_push: MAILBOX_PUSH_PROTOCOL,
            media: MEDIA_PROTOCOL,
            search: SEARCH_PROTOCOL,
            inventory: INVENTORY_PROTOCOL,
            digest: DIGEST_PROTOCOL,
            gossip_prefix: None,
        }
    }

    /// Empty means public. A name is at most 64 printable ASCII bytes, without edge spaces.
    /// The namespace is the first eight bytes of SHA-256("dyapp/network/" || name), in hex.
    pub fn new(name: &str) -> anyhow::Result<Self> {
        if name.is_empty() {
            return Ok(Self::public());
        }
        if let Some(c) = name.chars().find(|c| !(' '..='~').contains(c)) {
            anyhow::bail!("network name: {c:?} is not printable ASCII");
        }
        if name.len() > 64 {
            anyhow::bail!("network name: {} bytes, at most 64", name.len());
        }
        if name.starts_with(' ') || name.ends_with(' ') {
            anyhow::bail!("network name: leading or trailing space");
        }
        let hash = dyapp_identity::sha256(&[b"dyapp/network/", name.as_bytes()].concat());
        let namespace: String = hash[..8].iter().map(|b| format!("{b:02x}")).collect();
        let prefix = format!("/dyapp/net-{namespace}");
        let id = |name| StreamProtocol::try_from_owned(format!("{prefix}/{name}"));
        Ok(Self {
            kad: id("kad")?,
            search_kad: id("search-kad")?,
            node: id("node")?,
            profile: id("profile")?,
            mailbox: id("mailbox")?,
            mailbox_push: id("mailbox-push")?,
            media: id("media")?,
            search: id("search")?,
            inventory: id("inventory")?,
            digest: id("digest")?,
            gossip_prefix: Some(format!("{prefix}/meshsub")),
            version: prefix,
        })
    }

    /// Topic `<version>/profiles/<n>` of the search index.
    pub fn search_topic(&self, n: u32) -> gossipsub::IdentTopic {
        gossipsub::IdentTopic::new(format!("{}/profiles/{n}", self.version))
    }

    /// The nine existing protocol IDs; the reserved digest ID is not served yet.
    pub fn all(&self) -> [&StreamProtocol; 9] {
        [
            &self.kad,
            &self.search_kad,
            &self.node,
            &self.profile,
            &self.mailbox,
            &self.mailbox_push,
            &self.media,
            &self.search,
            &self.inventory,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_and_named_protocols_keep_stable_ids() {
        let public = Protocols::public();
        let ids: Vec<_> = public.all().into_iter().map(|id| id.as_ref()).collect();
        assert_eq!(
            ids,
            [
                "/dyapp/kad",
                "/dyapp/search-kad",
                "/dyapp/node",
                "/dyapp/profile",
                "/dyapp/mailbox",
                "/dyapp/mailbox-push",
                "/dyapp/media",
                "/dyapp/search",
                "/dyapp/inventory",
            ]
        );
        assert_eq!(public.version, "/dyapp");
        assert_eq!(public.digest.as_ref(), "/dyapp/digest");
        assert!(public.gossip_prefix.is_none());
        assert_eq!(
            public.search_topic(3).to_string(),
            crate::search_topic(3).to_string()
        );
        assert_eq!(Protocols::new("").unwrap().all(), public.all());

        let named = Protocols::new("test").unwrap();
        let prefix = "/dyapp/net-540fd23e9db6e2d1";
        assert_eq!(named.version, prefix);
        for id in named.all() {
            assert!(id.as_ref().starts_with(&format!("{prefix}/")));
        }
        assert_eq!(named.media.as_ref(), format!("{prefix}/media"));
        assert_eq!(named.digest.as_ref(), format!("{prefix}/digest"));
        assert_eq!(
            named.gossip_prefix.as_deref(),
            Some("/dyapp/net-540fd23e9db6e2d1/meshsub")
        );
        assert_eq!(
            named.search_topic(3).to_string(),
            format!("{prefix}/profiles/3")
        );
        assert_eq!(named.clone().all(), named.all());
    }

    #[test]
    fn names_validate_without_normalization() {
        Protocols::new(&"a".repeat(64)).unwrap();
        assert!(Protocols::new(&"a".repeat(65))
            .unwrap_err()
            .to_string()
            .contains("65"));
        Protocols::new("a b-c.D_9!").unwrap();
        for bad in ["a\nb", "a\x7fb", "сеть", " a", "a "] {
            assert!(Protocols::new(bad).is_err(), "{bad:?}");
        }
        assert!(Protocols::new("сеть")
            .unwrap_err()
            .to_string()
            .contains("'с'"));
        assert!(Protocols::new("a\nb")
            .unwrap_err()
            .to_string()
            .contains("'\\n'"));
        assert_ne!(
            Protocols::new("Net").unwrap().version,
            Protocols::new("net").unwrap().version
        );
    }
}
