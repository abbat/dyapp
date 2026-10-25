//! Validated protocol namespaces shared by swarm negotiation and protobuf codecs.

use crate::{
    DIGEST_PROTOCOL, IDENTIFY_PROTOCOL, INVENTORY_PROTOCOL, KAD_PROTOCOL, KEY_PACKAGE_PROTOCOL,
    MAILBOX_PROTOCOL, MAILBOX_PUSH_PROTOCOL, MEDIA_PROTOCOL, NODE_PROTOCOL, PROFILE_PROTOCOL,
    SEARCH_KAD_PROTOCOL, SEARCH_PROTOCOL,
};
use libp2p::{gossipsub, StreamProtocol};

/// IDs of one network. Named networks use a domain-separated hash rather than the raw name.
#[derive(Clone, Debug)]
pub struct Protocols {
    /// Identify version and prefix of every other ID: `/dyapp` on the public network.
    pub version: String,
    /// Store-role Kademlia.
    pub kad: StreamProtocol,
    /// Search-role Kademlia.
    pub search_kad: StreamProtocol,
    /// Node control requests.
    pub node: StreamProtocol,
    /// Profile publication and lookup.
    pub profile: StreamProtocol,
    /// Device KeyPackage publication and allocation.
    pub key_package: StreamProtocol,
    /// Mailbox store, fetch and ack.
    pub mailbox: StreamProtocol,
    /// Node-to-client mailbox notifications.
    pub mailbox_push: StreamProtocol,
    /// Media blob transfer.
    pub media: StreamProtocol,
    /// Search queries.
    pub search: StreamProtocol,
    /// Search index inventory exchange.
    pub inventory: StreamProtocol,
    /// Search bucket digests.
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
            key_package: KEY_PACKAGE_PROTOCOL,
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
        dyapp_identity::network::check_name(name).map_err(anyhow::Error::msg)?;
        let hash = dyapp_identity::sha256(&[b"dyapp/network/", name.as_bytes()].concat());
        let namespace: String = hash[..8].iter().map(|b| format!("{b:02x}")).collect();
        let prefix = format!("/dyapp/net-{namespace}");
        let id = |name| StreamProtocol::try_from_owned(format!("{prefix}/{name}"));
        Ok(Self {
            kad: id("kad")?,
            search_kad: id("search-kad")?,
            node: id("node")?,
            profile: id("profile")?,
            key_package: id("keypackage")?,
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

    /// The eleven served protocol IDs.
    pub fn all(&self) -> [&StreamProtocol; 11] {
        [
            &self.kad,
            &self.search_kad,
            &self.node,
            &self.profile,
            &self.key_package,
            &self.mailbox,
            &self.mailbox_push,
            &self.media,
            &self.search,
            &self.inventory,
            &self.digest,
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
                "/dyapp/keypackage",
                "/dyapp/mailbox",
                "/dyapp/mailbox-push",
                "/dyapp/media",
                "/dyapp/search",
                "/dyapp/inventory",
                "/dyapp/digest",
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
