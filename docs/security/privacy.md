# Privacy & Metadata Visibility

> **No privacy guarantees today.** Messages sent through `rust/client` are MLS ciphertext, but
> the app does not use it yet and metadata is visible; profiles are signed but public by design ([Encryption & Security Status](encryption.md)). This page
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

Source: `rust/bootstrap/src/service.rs`, `rust/bootstrap/src/storage.rs`.

| Data | Who sees it today | Protection today | Retention today | Target protection | Residual risk even in target |
|------|-------------------|------------------|-----------------|-------------------|------------------------------|
| Envelope `ciphertext` | Operator, the device holding the mailbox key | MLS ciphertext from `rust/client`; the node cannot check what other clients put | Until acked or the TTL (default 24 h; repair can extend it, see [Retention](#retention)) | E2E AEAD, only recipient decrypts; plaintext padded to the next power of two from 1 KiB | Size bucket and timing |
| Envelope `class` (message or signal) | Operator, mailbox replicas | None; the node needs it for the signal quota | Same as body | Same | Operator counts a mailbox's signals, not which kind |
| Mailbox address, sender key, put time | Operator | Only the device can fetch or ack ([bootstrap](../architecture/bootstrap.md#served-protocol)) | Same as body | Sealed sender: a fresh key per envelope, the sender only inside the ciphertext | Operator learns which mailbox gets mail and when, and the sender's IP |
| Profile fields (name (planned), age, country, location, income, kids, goals, gender, orientation, interests, photo hashes, …) | Operator, any client (`/dyapp/profile` `get`), network | Public by design, signed by the owner ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)); empty fields are not published | Until 30 days (`limits.profile_ttl_days`) after a newer profile publish or fresh signed heartbeat, tombstone included | Same; location is a place name (city or district), never coordinates, and optional (see below) | Everything published is readable by anyone and cannot be reliably withdrawn; combined with peer ID and IP it can identify a person |
| Profile `peer_id` | Operator, any client, network | Derived from the identity public key | Same as profile | Same | Stable identifier links all activity of one user |
| Client IP address | Operator (TCP connection), network | None | Not stored by app code (bootstrap has no logging); reverse proxies/hosting may log it | A client proxy or Tor is research, not designed | Nodes link the IP to identity on publish, heartbeat, fetch and media requests, so several profiles of one person are linkable; calls with verified contacts reveal IPs to the peer |
| Video frame content | Peer; network once transport exists | None: SFrame exists in `rust/video` but no media path uses it | Not stored | SFrame | — |
| Video frame sizes, timing and SFrame header | Peer and network path | None | Not stored | Unencrypted by design | Resolution, frame rate, call duration |
| SDP / ICE candidates | Whoever relays signaling; no signaling channel exists yet (bootstrap has no endpoint for it) | — | — | Encrypted signaling (not designed); relay-only ICE until the contact is verified, a setting for every call (planned) | IP addresses of verified contacts inside ICE candidates |

## Retention

The node config sets `limits.message_ttl_hours = 24`; an envelope is kept until the device acks
it or the TTL passes, and `dyappd` deletes expired envelopes every hour. Exception: mailbox
repair gives a repaired copy a new TTL, so an envelope its device never acks can outlive the TTL
([lifecycle](../architecture/bootstrap.md#mailbox-envelope)). A chat attachment
is kept until `limits.attachment_retention_hours` (default 168) passes, also deleted hourly;
recipients do not release it, so every device of the chat can still fetch it. Its preview travels
inside the encrypted message, and the client downloads a stranger's original only on a tap
(`attachment::auto_download`); blurring their preview is app work. A profile, tombstone included, is deleted hourly once its owner
has neither published a newer version nor sent a fresh identity-signed heartbeat for
`limits.profile_ttl_days` (default 30). A heartbeat advances liveness only to its signed time,
within 10 minutes of the node clock and newer than the stored time; replayed heartbeats,
stale republishes and media requests do not extend profile retention. The node thereby learns
roughly when each owner was last active. Media blobs listed in
an owner's `keep` stay until that list is replaced, the owner's signed media liveness is older
than `limits.profile_ttl_days`, or disk pressure evicts the keep. Signed keep time must be
within 10 minutes of the node clock; replay does not advance liveness. Shared blobs remain
while any other keep or unexpired attachment holds them
([Bootstrap](../architecture/bootstrap.md#served-protocol)). Backups and replicas
(if an operator adds them) keep their own copies.

Target: nothing is kept forever. The operator may delete any data at any time; under a full quota
the data of the longest inactive profiles goes first
([ADR 0009](../decisions/0009-message-delivery-and-storage.md)),
and profile deletion is a signed tombstone (implemented on a single node) that
nodes honour best effort, without a guarantee ([ADR 0011](../decisions/0011-best-effort-deletion.md)).

## Age and location

The age is self-declared and unverified. Clients and nodes reject a profile with an age of 1 to 17
(`rust/profile`), but nothing stops a minor from declaring 18 or leaving the age empty.

Search canonicalizes a supplied place against its embedded GeoNames country/place set.
A profile with no place, or an unknown country/place pair, is indexed in the country
bucket and is findable by country rather than by that place. On a full search node,
established profiles in this bucket are subsampled before named places; newcomers
remain subject to their separate quota ([search](../architecture/bootstrap.md#search)).
Omitting a place therefore reduces discoverability; it does not hide the public profile.

The whole profile is public by design
([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)):
every published field, including age, location and optional sensitive fields
such as mental-health indicators, is readable by anyone. That is a disclosure,
not "safe" data. The target design therefore needs:

- no field published unless the user fills it in;
- location published only as a place name (city or district), never as coordinates, and optional;
- a warning before publishing that replicated data cannot be reliably withdrawn
  ([ADR 0011](../decisions/0011-best-effort-deletion.md));
- documentation that peer ID + IP + age + location can deanonymize a user;
- photo metadata (EXIF, GPS, device) stripped on the client before any upload: `strip_metadata` in the core
  (see [FFI design](../architecture/ffi-design.md)); the app setting, on by default, and the call before
  upload are not wired yet;
- a heartbeat at most once a day at a random time, so `last_seen` does not track app use;
- push notifications (not designed) that only wake the app, with no sender or content.

Attacks and verdicts: [Threat Model](threat-model.md#traffic-analysis).

## Rules for other docs

Do not write "no metadata leaks", "cannot deanonymize" or "never stored".
Link here and state the bound that applies (which observer, which field).
