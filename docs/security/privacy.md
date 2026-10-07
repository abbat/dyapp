# Privacy & Metadata Visibility

> **No privacy guarantees today.** Message crypto is stubbed and message routes
> have no authentication; profiles are signed but public by design ([Encryption & Security Status](encryption.md)). This page
> lists who can see each piece of data now, and what the target design would
> change. Even the target design does not hide all metadata.

## Observers

| Observer | When |
|----------|------|
| Bootstrap operator | Anything sent to or stored by a bootstrap node |
| Any HTTP client of bootstrap | No auth on reads: can list and read every profile and inbox, delete any message |
| Network on path (ISP, Wi-Fi) | Plain HTTP to bootstrap today; direct peer traffic once transport exists |
| Peer you connect to | Your IP address and whatever you send (direct P2P/WebRTC reveals IPs by design) |
| Local device | Everything stored on it (no local encryption is implemented) |

## Field visibility

Source: `rust/bootstrap/src/api.rs`, `rust/bootstrap/src/storage.rs`,
`rust/video/src/encryption.rs`.

| Data | Who sees it today | Protection today | Retention today | Target protection | Residual risk even in target |
|------|-------------------|------------------|-----------------|-------------------|------------------------------|
| Message body (`encrypted_payload`) | Operator, any client (`GET /messages/peer/:id`), network | None (plaintext; crypto stub) | Until deleted by anyone; TTL not enforced | E2E AEAD, only recipient decrypts | Size and timing |
| `sender_id`, `recipient_id`, message `timestamp` | Operator, any client, network | None | Same as body | TLS to bootstrap; access control on reads | Operator always learns who messages whom and when (routing metadata) |
| Profile fields (age, country, location, income, kids, goals, orientation, interests, …) | Operator, any client (`GET /profiles`), network | Public by design, signed by the owner ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)); empty fields are not published | Until the owner publishes a tombstone; the tombstone is kept forever | Same; location precision chosen by the user, or off (see below) | Everything published is readable by anyone and cannot be reliably withdrawn; combined with peer ID and IP it can identify a person |
| Profile `peer_id` | Operator, any client, network | Derived from the identity public key | Same as profile | Same | Stable identifier links all activity of one user |
| Message `peer_id`s | Operator, any client, network | None, self-asserted | Same as messages | Derived from public key | Same |
| Client IP address | Operator (TCP connection), network | None | Not stored by app code (bootstrap has no logging); reverse proxies/hosting may log it | Optional relay/proxy (not designed) | Direct P2P and WebRTC reveal IPs to the other peer |
| Video frame content | Peer; network once transport exists | None (stub returns plaintext) | Not stored | Frame AEAD | — |
| Video `FrameHeader` (`frame_id`, `timestamp`, `width`, `height`, `nonce`, `tag`) | Peer and network path | None | Not stored | Unencrypted by design | Resolution, frame rate, call duration |
| SDP / ICE candidates | Whoever relays signaling; no signaling channel exists yet (bootstrap has no endpoint for it) | — | — | Encrypted signaling (not designed) | IP addresses inside ICE candidates |

## Retention

`BootstrapConfig` sets `message_ttl_hours = 24`, and every message gets
`ttl_expires_at`. **Nothing enforces it**: `BootstrapStore::cleanup_expired`
has no callers outside tests, and read endpoints do not filter expired
messages. Messages stay until someone calls `DELETE`, and any client can do that
because there is no authentication. Profiles have no TTL: the latest signed
version stays until the owner replaces it with a tombstone, which the node keeps
so older versions are not re-imported. RocksDB backups and replicas (if an
operator adds them) keep their own copies.

Target: bootstrap storage is an LRU cache evicting the data of the longest
inactive profiles ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)),
and profile deletion is a signed tombstone (implemented on a single node) that
nodes honour best effort, without a guarantee ([ADR 0011](../decisions/0011-best-effort-deletion.md)).

## Age and location

The whole profile is public by design
([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)):
every published field, including age, location and optional sensitive fields
such as mental-health indicators, is readable by anyone. That is a disclosure,
not "safe" data. The target design therefore needs:

- no field published unless the user fills it in;
- location precision chosen by the user (exact, coarse, or off);
- a warning before publishing that replicated data cannot be reliably withdrawn
  ([ADR 0011](../decisions/0011-best-effort-deletion.md));
- documentation that peer ID + IP + age + location can deanonymize a user.

## Rules for other docs

Do not write "no metadata leaks", "cannot deanonymize" or "never stored".
Link here and state the bound that applies (which observer, which field).
