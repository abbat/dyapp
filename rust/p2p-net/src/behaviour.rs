//! The swarm behaviour set.

// The NetworkBehaviour derive emits `BehaviourEvent` without docs.
#![allow(missing_docs)]
use super::{
    IncomingGroupGuard, InventoryBehaviour, KeyPackageBehaviour, MailboxBehaviour, MediaBehaviour,
    NodeBehaviour, ProfileBehaviour, PushBehaviour, SearchBehaviour,
};
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{autonat, connection_limits, gossipsub, identify, kad, memory_connection_limits};

/// Every behaviour of a client or node swarm.
#[derive(NetworkBehaviour)]
pub struct Behaviour {
    /// Global and per-peer connection counts.
    pub limits: connection_limits::Behaviour,
    /// Refuses new connections above the memory limit.
    pub memory: memory_connection_limits::Behaviour,
    /// Incoming connections per remote IP group, when configured.
    pub incoming_groups: Toggle<IncomingGroupGuard>,
    /// Store-role Kademlia over [`KAD_PROTOCOL`](crate::KAD_PROTOCOL).
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
    /// Identify, with [`IDENTIFY_PROTOCOL`](crate::IDENTIFY_PROTOCOL).
    pub identify: identify::Behaviour,
    /// External address confirmation.
    pub autonat: autonat::Behaviour,
    /// Node control requests.
    pub node: NodeBehaviour,
    /// Profile publication and lookup.
    pub profile: ProfileBehaviour,
    /// Device KeyPackage publication and allocation.
    pub key_package: KeyPackageBehaviour,
    /// Mailbox store, fetch and ack.
    pub mailbox: MailboxBehaviour,
    /// Node to client only: a node sends, a client receives.
    pub push: PushBehaviour,
    /// Media blob transfer.
    pub media: MediaBehaviour,
    /// Server mode on search nodes only; others look search nodes up as clients.
    pub search_kad: kad::Behaviour<kad::store::MemoryStore>,
    /// Search nodes only.
    pub gossip: Toggle<gossipsub::Behaviour>,
    /// Inbound on search nodes only.
    pub search: SearchBehaviour,
    /// Search index inventory exchange.
    pub inventory: InventoryBehaviour,
}
