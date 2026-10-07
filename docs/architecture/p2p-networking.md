# P2P Networking Architecture

## Overview

Peer-to-peer networking using Quinn (QUIC protocol) with multi-strategy peer discovery.
This describes the current code. The target is rust-libp2p with Sybil and eclipse
defences [ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md).

**Key principles:**
- Direct peer connections where possible (NAT permitting)
- Fallback to bootstrap relay for NAT-blocked connections
- Gossip protocol for peer distribution
- Reputation system for trust

## Components

### Peer (peer.rs)

Represents a remote peer in the network:

```rust
pub struct Peer {
    pub id: PeerId,                  // Public key hash (unique identifier)
    pub addresses: Vec<SocketAddr>,  // Known network addresses
    pub public_key: String,          // Ed25519 public key (hex)
    pub verified: bool,              // TOFU verification status
    pub last_seen: Option<i64>,      // Unix millis
    pub reputation: i32,             // Trust score [-100, 100]
}
```

**Reputation mechanics:**
- Default: 0
- Successful message delivery: +10
- Failed delivery: -20
- Malicious behavior (bad signature): -50
- Trust threshold: reputation > -50 AND verified

### PeerDiscovery (discovery.rs)

Manages peer database and discovery strategies:

```rust
pub enum DiscoveryStrategy {
    Bootstrap,  // Query centralized index
    Gossip,     // Peer shares known peers
    DHT,        // Distributed hash table
    Hybrid,     // Try all strategies
}
```

**Operations:**
- `add_peer()` — register discovered peer
- `get_reachable_peers()` — filter peers with known addresses
- `get_trusted_peers()` — filter verified + good reputation
- `query_bootstrap()` — async query bootstrap server
- `get_peers_to_share()` — gossip up to 10 peers to a peer

**Hybrid discovery flow:**
1. Try direct connection to known address
2. Gossip: ask other peers if they know the target
3. Query bootstrap index for peer address
4. If still not found, use bootstrap signaling relay

### PeerConnection (connection.rs)

Per-peer connection state machine:

```rust
pub enum ConnectionState {
    Disconnected,    // Not connected
    Connecting,      // Handshake in progress
    Connected,       // QUIC connection established
    Authenticated,   // Identity verified (X25519 key exchange done)
    Failed,          // Connection failed / broken
}
```

**Metrics tracked:**
- Messages sent/received
- Bytes sent/received
- Connection duration
- Estimated bandwidth (kbps)
- Idle detection (no activity timeout)

### QuicTransport (transport.rs)

Quinn/QUIC configuration:

```rust
pub struct TransportConfig {
    pub connection_timeout_ms: u64,      // How long to wait for handshake
    pub idle_timeout_ms: u64,            // When to drop idle connection
    pub max_concurrent_streams: u64,     // Bidirectional stream limit
    pub keep_alive_enabled: bool,        // Mobile-specific
    pub keep_alive_interval_secs: u32,   // Heartbeat interval
}
```

**Presets:**
- `default_for_mobile()`: 30s timeout, 60s idle, keep-alive enabled, 16 streams
- `default_for_desktop()`: 10s timeout, 120s idle, keep-alive disabled, 64 streams

## Connection Establishment

```
Client                          Peer
  |                             |
  |--- QUIC Initial Packet ----->|
  |                             | (verify source IP)
  |<-- QUIC Handshake Response --|
  |                             |
  |--- X25519 Key Exchange ------|  (planned)
  |<-- Signature Verification ---|  (Ed25519, planned)
  |                             |
  |--- State: Connected -------->|
  |     (Authenticated)          |
  |                             |
  |==== Encrypted Channel =====>|
  |<=== Encrypted Channel ======|
```

## NAT Traversal Strategy (ICE-like)

1. **Direct connection** (best case)
   - Try connecting to all known peer addresses
   - If one works, use it

2. **STUN-like public address discovery**
   - Ask bootstrap server: "What's my public IP?"
   - Bootstrap echoes client's source address

3. **TURN-like relay fallback**
   - Bootstrap acts as relay for packets (encrypted once crypto exists)
   - Client sends to bootstrap, relay forwards to peer
   - Higher latency but works behind strict NAT

4. **Gossip assistance**
   - If direct fails, ask other peers for relay help
   - Peer A asks Peer B to relay messages to Peer C

## Peer Discovery Protocol

### Gossip Message

```
{
  "type": "peer_share",
  "peers": [
    {
      "id": "peer_hash_123",
      "addresses": ["203.0.113.1:9000", "203.0.113.1:9001"],
      "public_key": "abc123...",
      "reputation": 45
    },
    ...
  ]
}
```

Sent periodically (or on demand) to known peers.

### Bootstrap Query (planned, not implemented)

Bootstrap has no `/api/peers` or search endpoint; today a client can only page
through all signed profiles with `GET /profiles?skip=&limit=` (Protobuf `ProfileList`) and filter locally (see
[Bootstrap REST API](bootstrap.md#rest-api)). Filters can use any public
profile field; income is a from–to range compared within one country
([ADR 0012](../decisions/0012-private-p2p-interactions.md)). Sketch of the
intended query:

```
GET /api/peers?age_range=30-45&country=DE&income_from=50000&income_to=80000
Content-Type: application/json

Response:
{
  "profiles": [
    {
      "user_id": "hash",
      "addresses": ["addr1", "addr2"],
      "profile": "<signed profile record>"
    }
  ]
}
```

Bootstrap returns signed profile records as stored; profiles are public by design, so they stay plaintext ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)).

## Security (target design)

> ⚠️ Not implemented: `QuicTransport` bind/connect/listen are TODO no-ops and
> signatures are stubs. See [Encryption & Security Status](../security/encryption.md).

- **Peer addresses** — app code has no logging today; direct connections reveal IPs to the other peer by design ([Privacy](../security/privacy.md))
- **Reputation system** — penalizes bad behavior
- **Signature verification** — all messages signed with Ed25519
- **libp2p transport security** — device key separate from the identity key ([ADR 0006](../decisions/0006-transport-keys-separate-from-identity.md))
- **MLS** — end-to-end keys and user authentication ([ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md))

## IPv6 / IPv4 Handling

- Prefer IPv6 addresses (modern stacks)
- Fallback to IPv4 if IPv6 fails
- Dual-stack bindings where possible
- Link-local addresses filtered (not routable)

## Testing

Unit tests included:
- Peer reputation mechanics
- Trusted peer filtering
- Connection state transitions
- Bandwidth calculations
- Idle detection

Integration tests (future):
- Multi-peer gossip
- NAT traversal scenarios
- Bootstrap fallback
