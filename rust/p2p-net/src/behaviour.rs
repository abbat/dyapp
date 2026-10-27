//! The swarm behaviour set.

// The NetworkBehaviour derive emits `BehaviourEvent` without docs.
#![expect(
    missing_docs,
    reason = "NetworkBehaviour derive emits an undocumented event type"
)]
use super::{
    Announce, DigestBehaviour, Endpoints, IncomingGroupGuard, InventoryBehaviour,
    KeyPackageBehaviour, MailboxBehaviour, MediaBehaviour, NodeBehaviour, PowGate,
    ProfileBehaviour, PushBehaviour, SearchBehaviour,
};
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{autonat, connection_limits, dcutr, gossipsub, kad, memory_connection_limits, relay};

/// Every behaviour of a client or node swarm.
#[derive(NetworkBehaviour)]
pub struct Behaviour {
    /// Global and per-peer connection counts.
    pub limits: connection_limits::Behaviour,
    /// Refuses new connections above the memory limit.
    pub memory: memory_connection_limits::Behaviour,
    /// Incoming connections per remote IP group, when configured.
    pub incoming_groups: Toggle<IncomingGroupGuard>,
    /// Refuses outbound peers without node-ID proof of work, on nodes with a dial guard.
    pub pow_gate: Toggle<PowGate>,
    /// The remote IP of each connection, resolved for DNS dials.
    pub endpoints: Endpoints,
    /// Store-role Kademlia over [`KAD_PROTOCOL`](crate::KAD_PROTOCOL).
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
    /// Identify, with [`IDENTIFY_PROTOCOL`](crate::IDENTIFY_PROTOCOL), announcing only preferred
    /// listen addresses.
    pub identify: Announce,
    /// External address confirmation.
    pub autonat: autonat::Behaviour,
    /// Circuit relay v2 client: reservations on relays and dials through circuits.
    pub relay_client: relay::client::Behaviour,
    /// Circuit relay v2 hop protocol, on relay nodes only.
    pub relay: Toggle<relay::Behaviour>,
    /// Hole punching that upgrades a relayed connection to a direct one.
    pub dcutr: dcutr::Behaviour,
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
    /// Search bucket digests.
    pub digest: DigestBehaviour,
}
