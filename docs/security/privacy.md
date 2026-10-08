# Privacy & Metadata Visibility

> **No privacy guarantees today.** Message crypto is stubbed, so the operator reads message
> bodies; profiles are signed but public by design ([Encryption & Security Status](encryption.md)). This page
> lists who can see each piece of data now, and what the target design would
> change. Even the target design does not hide all metadata.

## Observers

| Observer | When |
|----------|------|
| Bootstrap operator | Anything sent to or stored by a bootstrap node |
| Any libp2p client of bootstrap | Can read every profile and put into any mailbox; cannot read or ack another device's mailbox |
| Network on path (ISP, Wi-Fi) | Connection metadata only: libp2p traffic is Noise or TLS 1.3 encrypted |
| Peer you connect to | Your IP address and whatever you send (direct P2P/WebRTC reveals IPs by design) |
| Local device | Everything stored on it (no local encryption is implemented) |

## Field visibility

Source: `rust/bootstrap/src/service.rs`, `rust/bootstrap/src/storage.rs`,
`rust/video/src/encryption.rs`.

| Data | Who sees it today | Protection today | Retention today | Target protection | Residual risk even in target |
|------|-------------------|------------------|-----------------|-------------------|------------------------------|
| Envelope `ciphertext` | Operator, the device holding the mailbox key | None (plaintext; crypto stub) | Until acked or the TTL (default 24 h) | E2E AEAD, only recipient decrypts | Size and timing |
| Mailbox address, sender device key, put time | Operator | Only the device can fetch or ack ([bootstrap](../architecture/bootstrap.md#served-protocol)) | Same as body | Same | Operator learns which keys write to which mailbox and when (routing metadata) |
| Profile fields (age, country, location, income, kids, goals, orientation, interests, photo hashes, …) | Operator, any client (`/dyapp/profile` `get`), network | Public by design, signed by the owner ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)); empty fields are not published | Until the owner publishes a tombstone, which is kept forever (target: retention TTL, default 30 days) | Same; location is a place name (city or district), never coordinates, and optional (see below) | Everything published is readable by anyone and cannot be reliably withdrawn; combined with peer ID and IP it can identify a person |
| Profile `peer_id` | Operator, any client, network | Derived from the identity public key | Same as profile | Same | Stable identifier links all activity of one user |

| Client IP address | Operator (TCP connection), network | None | Not stored by app code (bootstrap has no logging); reverse proxies/hosting may log it | Optional relay/proxy (not designed) | Direct P2P and WebRTC reveal IPs to the other peer |
| Video frame content | Peer; network once transport exists | None (stub returns plaintext) | Not stored | Frame AEAD | — |
| Video `FrameHeader` (`frame_id`, `timestamp`, `width`, `height`, `nonce`, `tag`) | Peer and network path | None | Not stored | Unencrypted by design | Resolution, frame rate, call duration |
| SDP / ICE candidates | Whoever relays signaling; no signaling channel exists yet (bootstrap has no endpoint for it) | — | — | Encrypted signaling (not designed) | IP addresses inside ICE candidates |

## Retention

The node config sets `limits.message_ttl_hours = 24`; an envelope is kept until the device acks
it or the TTL passes, and `dyapp-node` deletes expired envelopes every hour. Profiles have no TTL: the latest signed
version stays until the owner replaces it with a tombstone, which the node keeps
so older versions are not re-imported. Backups and replicas (if an
operator adds them) keep their own copies.

Target: nothing is kept forever. The operator sets a retention TTL (default 30
days, tombstones included) and may delete any data at any time; under a full quota
the data of the longest inactive profiles goes first
([ADR 0009](../decisions/0009-message-delivery-and-storage.md)),
and profile deletion is a signed tombstone (implemented on a single node) that
nodes honour best effort, without a guarantee ([ADR 0011](../decisions/0011-best-effort-deletion.md)).

## Age and location

The whole profile is public by design
([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)):
every published field, including age, location and optional sensitive fields
such as mental-health indicators, is readable by anyone. That is a disclosure,
not "safe" data. The target design therefore needs:

- no field published unless the user fills it in;
- location published only as a place name (city or district), never as coordinates, and optional;
- a warning before publishing that replicated data cannot be reliably withdrawn
  ([ADR 0011](../decisions/0011-best-effort-deletion.md));
- documentation that peer ID + IP + age + location can deanonymize a user.

## Rules for other docs

Do not write "no metadata leaks", "cannot deanonymize" or "never stored".
Link here and state the bound that applies (which observer, which field).
