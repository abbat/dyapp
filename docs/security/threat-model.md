# Threat Model

Who can attack the network and its nodes, what they are after, and which defence covers each
threat. **Status** says whether the defence exists in code today; [Encryption & Security
Status](encryption.md) remains the source of truth for crypto claims and
[Privacy](privacy.md) for who sees which field.

## Assets

| Asset | Property we want |
|-------|------------------|
| Message content | Only the recipient's devices read it |
| Public profile | Only the owner publishes or changes it; anyone may read it |
| Mailbox | Only the device that owns it reads or deletes its envelopes |
| Identity key, device keys | Never leave the device |
| Metadata (who writes to whom, when, from which IP) | As little as possible leaks; full hiding is not a goal |
| Availability | A user can publish, fetch and deliver while some honest nodes are reachable |
| Node resources (disk, traffic, CPU, connections) | One abuser cannot exhaust them or knock the node over |

## Attackers

| Attacker | Can |
|----------|-----|
| **Node operator** | Read everything stored on or sent to their node; drop, delay or withhold data; serve an older signed profile; log IPs and timing |
| **Network observer** (ISP, Wi-Fi) | See connection endpoints, sizes and timing; tamper with packets |
| **Malicious client or peer** | Send any request to any node: floods, oversized or malformed messages, forged signatures, replayed requests, writes into strangers' mailboxes |
| **Sybil** | Create unlimited libp2p peer IDs and identity keys for free, run many nodes, surround a key in the DHT (eclipse) |
| **Compromised device** | Everything that device's keys allow |
| **Hostile contributor** | Open pull requests and issues, write text that maintainers and AI agents read |
| **Supply-chain attacker** | Publish or take over a dependency, base image, CI action or a maintainer account |

## Threats and defences

| Threat | Defence | Status |
|--------|---------|--------|
| Observer reads or tampers with traffic | libp2p QUIC (TLS 1.3) and TCP + Noise | ✅ Implemented ([p2p-net](../architecture/p2p-networking.md)) |
| Operator or observer reads messages | MLS end-to-end encryption ([ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md)) | ❌ Planned: payload crypto is a stub |
| Forged or rolled-back profile | Ed25519 signature by the owner; a node rejects an older version | ✅ Implemented; a node can still serve an older version it kept |
| Another client reads or empties a mailbox | Fetch and ack signed by the device key over a nonce issued for this connection | ✅ Implemented ([mailboxes](../architecture/bootstrap.md#mailboxes)) |
| Replayed mailbox request | The nonce is bound to one libp2p connection; other requests are idempotent | ✅ Implemented |
| Spam into a mailbox | Size limits per envelope and mailbox, puts limited per sender key and IP group | ✅ Implemented; the sender key is free, so the IP group is the real bound |
| Request floods | Token buckets per peer ID and IP group, local ban after repeated refusals | ✅ Implemented ([rate limiting](../architecture/bootstrap.md#rate-limiting)) |
| Disk, traffic or connection exhaustion | Store size caps, free-space floor, byte rate, connection, stream and memory limits | ✅ Implemented ([resource guards](../architecture/bootstrap.md#resource-guards)) |
| Oversized or malformed messages | Per-protocol wire caps: 1 MiB + 64 KiB, or 6 MiB + 64 KiB for media, protobuf decoding, unknown variants answered `UNSUPPORTED` | ✅ Implemented |
| Operator withholds or drops data | R = 5 replicas on independent nodes; the client tries another replica ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)) | ❌ Planned: a node stores only what it is sent |
| Abuser keeps using one node | Operator deny list of peer IDs, IP groups and key hashes | ✅ Implemented ([deny list](../architecture/bootstrap.md#deny-list)); a key is free to regenerate, so IP-group quotas carry the rest |
| Sybil eclipse of a key or a node | Node-ID proof of work, routing-table IP diversity, disjoint lookups, replicas at H(key ‖ i), local reputation ([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)) | ❌ Planned: the DHT is plain Kademlia |
| No way in without one seed | Several seeds, DNS seed names, a cache of peers seen before | ⚠️ Partial: seeds, the node's peer cache and anchors exist; no list ships ([joining](../architecture/p2p-networking.md)) |
| Search index flooded with fake profiles | Profile proof of work bound to the owner's key ([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)) | ✅ Implemented: search nodes index and relay only profiles with enough proof of work ([search](../architecture/bootstrap.md#search)) |
| Metadata: who talks to whom | Mailbox addresses are device keys, not user IDs; delivery receipts travel as ordinary encrypted messages | ⚠️ Partial: the operator still sees which sender key writes to which mailbox, and IPs ([privacy](privacy.md)) |
| Stolen identity or device key | Keys stay on the device, separate transport keys ([ADR 0006](../decisions/0006-transport-keys-separate-from-identity.md)); MLS post-compromise security | ❌ Planned: no key storage, no MLS |
| Operator links peer IDs to IPs | — | Not defended: no relay or proxy is designed |

## Threat classes

Every class, rated impact × likelihood for the project today and ordered by criticality. Each
class gets its own analysis that decides fix or accept per attack; the table above lists the
defences already in place. Nodes are untrusted by design
([ADR 0007](../decisions/0007-open-bootstrap-network.md)).

| # | Class | Impact | Likelihood | Status |
|---|-------|--------|------------|--------|
| 1 | Weak cryptography: primitives, nonces, entropy, key storage | Critical | Certain: payload crypto is a stub | ❌ Planned |
| 2 | Supply chain: dependencies, build, CI, releases, update channel | Critical | High | ⚠️ Partial: lock files, dependency audit, pinned CI; the age check and signed releases are planned ([below](#supply-chain)) |
| 3 | MITM on end-to-end keys and on joining (seed list, DNS) | Critical | Medium | ⚠️ Partial: transport is authenticated; key verification and seed pinning are planned ([below](#mitm-and-key-verification)) |
| 4 | Traffic analysis and deanonymization of users | Critical | High | ⚠️ Partial: transport encryption, E2E signals; sealed sender, padding and relay-only calls are planned ([below](#traffic-analysis), [privacy](privacy.md)) |
| 5 | Spoofing: impersonated users, lookalike names and peer IDs, fake nodes | High | High | ⚠️ Partial: owner signatures, node-ID proof of work; text rules, local names and checked search results are planned ([below](#spoofing)) |
| 6 | Hijack of AI agents and developer tooling (prompt injection) | Critical | Medium | ❌ Planned |
| 7 | Hostile pull requests | Critical | Medium | ❌ Planned |
| 8 | Hostile node: withholding, censorship, search poisoning, harvesting | High | High | ⚠️ Partial (rows above) |
| 9 | Denial of service beyond the current guards: CPU asymmetry, crafted payloads | High | High | ⚠️ Partial (rows above) |
| 10 | Homoglyphs and Unicode tricks: hidden code, typosquatting, lookalike domains and apps | High | Medium | ❌ Planned |
| 11 | Social engineering and operations: phishing, account takeover, fake releases | High | Medium | ❌ Planned |
| 12 | Privilege escalation and host blast radius of a compromised node | High | Medium | ❌ Planned |
| 13 | Disclosure of node internals: private addresses, versions, logs, metrics | Medium | High | ❌ Planned |
| 14 | Illegal content planted on nodes, legal pressure on operators | High | Medium | ⚠️ Partial: deny list |
| 15 | Data tampering: media, deny lists, signature encoding, rollback | High | Medium | ⚠️ Partial: owner signatures, version checks |
| 16 | Client-side exposure: backups, notifications, logs | High | Medium | ❌ Planned |
| 17 | Reflection: an honest node sends traffic to a third party | Medium | Medium | ❌ Planned |
| 18 | Injection: SQL, paths, logs, commands, rendering | High | Low | ⚠️ Partial: bound SQL parameters |
| 19 | Replay | Medium | Low | ⚠️ Partial (rows above) |
| 20 | Downgrade and weak-parameter negotiation | High | Low | ⚠️ Partial: both transports are strong; version negotiation is not designed |
| 21 | Side channels and timing | Medium | Low | ❌ Planned |
| 22 | Clock manipulation against freshness windows and TTLs | Medium | Low | ⚠️ Partial: 10-minute windows |

### Cryptography

Class 1 in detail. Keys and rules: [Encryption & Security Status](encryption.md#keys).

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Stubs taken for crypto: payload passed through, `sign` returns the message, XOR key derivation, placeholder keys, random unchecked frame tags | `rust/messaging/src/encryption.rs`, `rust/video/src/encryption.rs`, FFI `generate_keypair` | Critical × certain | Fix: MLS, frame encryption and identity storage replace and delete the stubs |
| Forged, malleable or weak-key Ed25519 signature | Any `SignedRecord` | Critical × low | Accept: `verify_strict` rejects non-canonical and small-order input; every check goes through `dyapp-identity` |
| A signature of one kind replayed as another | Signed records; the node key signs deny lists and libp2p handshakes | High × low | Accept: versioned labels ending in `\0` are prefix-free and differ from libp2p's own labels; fix: a test pins it |
| One key serving two purposes | Identity, device, MLS and libp2p keys | High × medium | Fix: one key per purpose, listed in [keys](encryption.md#keys) |
| MLS ciphersuite downgrade or unknown suite | KeyPackages and Welcomes from untrusted nodes | Critical × medium | Fix: only 0x0001 is accepted, nothing is negotiated |
| A group member whose credential does not chain to the Peer ID | MLS Add, Welcome | Critical × medium | Fix: checked on every Add and Welcome; verifying the Peer ID itself is class 3 |
| Frame nonce reuse under one key | Video frames | Critical × high with random nonces | Fix: SFrame ([RFC 9605](https://www.rfc-editor.org/rfc/rfc9605)), a counter nonce per sender key, keys from the MLS exporter |
| Attachment key or nonce reuse, a guessable release secret | Media blobs | High × medium | Fix: a fresh random key per attachment, counter nonces, a 32-byte release secret |
| Weak or failed randomness | Key generation, nonces, secrets | Critical × low | Accept: only the OS generator (`getrandom`), which blocks until seeded and fails closed; a cloned VM relies on the kernel's VM generation ID (Linux 6.1 on Debian 12) |
| Identity or device key read from disk or backups | Client storage | Critical × medium | Fix: wrapped by the platform keystore, excluded from backups and cloud sync |
| Secret copies left in memory | `Identity::secret`, key generation, key file buffers | Medium × low | Fix: wiped after use; swap and core dumps of a compromised OS are out of scope |
| Node key read from disk or backups | `node.key` | Medium × medium | ⚠️ Partial: created with mode 0600; fix: refuse a key readable by group or others; backups made with `umask 077` |
| MLS state read from a stolen device | Client database | High × low | Accept: platform data protection (iOS file protection, Android credential-encrypted storage) and desktop disk encryption; flash may keep deleted epoch secrets |
| Proof-of-work parameters too cheap or too costly to check | Profile Argon2id (8 MiB, 1 pass), node-ID SHA-256 | Medium × low | Accept: pinned by tests; the cost to a checking node is class 9 |
| Harvest now, decrypt later | X25519 in 0x0001, Ed25519 | High × medium | Accept for now: neither is post-quantum; a hybrid MLS ciphersuite is planned when openmls ships an audited one |
| Algorithm migration opens a downgrade | Domain labels, ciphersuite | High × low | Accept: a new algorithm comes with a new label or suite and a dated cut-over, never by negotiation; negotiation in general is class 20 |
| Timing leaks in crypto code | Signatures, AEAD | Medium × low | Out of scope here: class 21 |

### MITM and key verification

Class 3 in detail. A Peer ID is the hash of the identity key
([ADR 0004](../decisions/0004-identity-keys.md)), so whoever holds a contact's Peer ID can check
the identity key without trusting any node. The identity key never changes: a new key is a new
Peer ID and a new contact. What a node can still substitute is everything below the identity key.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| A node hands out another identity key for a known Peer ID | Profile fetch | Critical × low | Accept: the key must hash to the Peer ID and sign the profile |
| A cloned profile (another key, the same name and photo) in search | Search results | High × high | Out of scope here: spoofing, class 5 |
| A node serves an older signed profile that still lists a removed or stolen device | Profile fetch, any replica | Critical × medium | Fix: the client keeps the highest profile version seen per contact, asks several replicas and takes the highest |
| A node serves a KeyPackage of a removed device, or a stale one | KeyPackage fetch | Critical × medium | Fix: a KeyPackage is used only for a device in the newest known profile, and expires after 30 days; devices republish on their heartbeat |
| A node hands one single-use KeyPackage to several senders | KeyPackage fetch | Low × medium | Accept: the second Welcome fails, which is denial of service; the last-resort KeyPackage weakens forward secrecy of the first message ([ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md)) |
| A stolen identity key adds an attacker's device to the profile | Profile publish | Critical × low | Fix: every chat with the contact shows "new device"; a verified contact loses its verified mark until checked again |
| First contact through a node, not out of band | Search, forwarded contact | Critical × medium | Fix: the safety number or QR code is derived from both Peer IDs; until it is compared the contact is shown as unverified |
| A node in the middle of linking a new device | Device linking | Critical × medium | Fix: keys and history travel only over a channel keyed by a secret in the QR code; nodes carry ciphertext only |
| Swapped DTLS fingerprints in call signaling, a TURN relay in the middle | Video signaling, TURN | Critical × medium | Fix: offer and answer travel only as MLS messages; TURN sees SRTP only ([Video](../architecture/video.md)) |
| Another node answers on a peer's address | libp2p dial | High × low | Accept: TLS and Noise bind the connection to the libp2p key; a dial with a peer ID refuses any other key |
| A hijacked DNS name, domain or seed host: a new node or client sees only the attacker's nodes (eclipse) | Seeds, DNS | Critical × medium | Fix: a seed may carry a peer ID and a node with another key is refused; the client's built-in list always does; the client asks several seeds and cross-checks with disjoint lookups |
| A fake node answers every protocol | Client to node | High × medium | Accept: it cannot forge signed or MLS data, only withhold it, which is class 8 |
| Lookalike Peer IDs, names or domains | Contact list, links | High × medium | Out of scope here: classes 5 and 10 |

### Supply chain

Class 2 in detail. In place: `Cargo.lock` with `--locked` builds, `cargo deny` (crates.io only, no
git sources, yanked crates refused, OpenSSL banned), `cargo audit` weekly and on every change,
GitHub Actions and Docker build images pinned by digest, downloaded tools and the Gradle wrapper
checked by SHA-256, and an SPDX SBOM per CI run.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| A malicious or hijacked crate version, a typosquat, a `build.rs` or proc macro that runs at build time | Cargo dependencies | Critical × medium | Fix: no version younger than 14 days, enforced by a `make quality` check against the crates.io index and a 14-day Dependabot cooldown (today 7); a new dependency is reviewed by hand |
| A malicious Gradle artifact | Android dependencies | Critical × medium | Fix: Gradle dependency verification (`verification-metadata.xml` with SHA-256) |
| An unpinned tool pulled in CI: `cargo install cargo-deny` in CodeQL, `pip install flake8`, xcodegen without a checksum, `dtolnay/rust-toolchain` at a branch, `debian:bookworm-slim` by tag | CI workflows, test Dockerfiles | High × medium | Fix: pinned versions with `--locked`, checksums and digests |
| A pull request poisons the GitHub Actions build cache read by `main` | Docker builds with the GHA cache | High × medium | Fix: only pushes to `main` write the cache, pull requests read it; every workflow sets `permissions` at the top |
| A tampered Debian package from the mirror | apt in build images | Medium × low | Accept: Debian repositories are signed; versions are not pinned |
| A forged or tampered release: deb, APK, app bundle, container image | Downloads, image registry | Critical × medium | Fix (no release pipeline yet): release artifacts get Sigstore keyless signatures, SLSA build provenance and the SBOM; [Deployment](../operations/deployment.md#updates) documents how to verify them |
| An operator rolls back to a vulnerable release | Node updates | Medium × low | Accept: the operator's choice; signatures prove origin, not freshness |
| A hostile pull request | Repository | Critical × medium | Out of scope here: class 7 |
| A hijacked AI agent or maintainer account | Developer tooling, GitHub account | Critical × medium | Out of scope here: classes 6 and 11 |

### Traffic analysis

Class 4 in detail; who sees which field: [Privacy](privacy.md). In place: libp2p traffic is
encrypted, likes and views are end-to-end signals a node cannot tell from messages
([ADR 0012](../decisions/0012-private-p2p-interactions.md)), presence goes only to allowed
peers. Today an envelope is signed by the sender's device key, which the sender's public profile
lists, and the mailbox is the hash of the recipient's listed device key, so a node operator sees
who writes to whom and when.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| An operator maps the envelope's sender key and the mailbox to two public profiles: the social graph | Stored envelopes | Critical × high | Fix: sealed sender. Each envelope is signed by a fresh one-off key; the sender's identity is only inside the MLS ciphertext. Per-key quotas then bound nothing; the IP-group and mailbox quotas remain |
| An operator learns whose mailbox an envelope is for | Mailbox address | Medium × high | Accept: the recipient is known, as in Signal; the mailbox hash is derived from the public device list |
| Envelope size tells a like or view from text or a photo message | Stored envelopes, network | Medium × high | Fix: the plaintext is padded to the next power of two from 1 KiB before encryption |
| Send and delivery times correlate two users | Node, network | Medium × medium | Accept: batching would delay chats; a global passive observer is out of scope |
| Heartbeats and `last_seen` track when a user opens the app | Profile replicas | Medium × high | Fix: the client sends a heartbeat at most once a day at a random time, not on every app open; the 30-day profile TTL allows it |
| The replica holding an online device's connection sees its time online | Mailbox connection | Medium × medium | Accept: needed for delivery |
| An operator links a client IP to an identity: publish, heartbeat, mailbox fetch, media keep | Every request | High × high | Accept: named residual; a client proxy or Tor for the TCP transport is research, together with DPI resistance |
| Fetching another profile's photos or searching tells a node who looks at whom and for what | Media and search requests | Medium × high | Accept: same residual and research as the IP above |
| Several profiles of one person are linked by IP and timing | Every request | High × medium | Accept: documented; separate profiles are unlinked identities only for other users |
| Mass harvesting of public profiles | Profile `get`, search | High × high | Accept: profiles are public by design ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)); rate limits and hostile nodes are classes 9 and 8. Fix: a warning before publishing that published fields cannot be withdrawn |
| EXIF and GPS metadata in a photo reveal coordinates or the device | Profile and chat photos | High × high | Fix: the client strips metadata from every image before upload |
| ICE candidates reveal a caller's IP to the other side | Calls | High × medium | Fix: until a contact is verified, calls use only TURN relays (relay-only ICE); a setting forces relays for every call |
| Push notifications through APNs or FCM reveal sender and content to Apple or Google | Push (not designed) | High × medium | Fix (requirement for the design): a push only wakes the app, without sender or content |
| Colluding or Sybil nodes take every replica of a user | DHT | High × medium | Out of scope here: class 8 ([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)) |
| A network observer fingerprints dyapp traffic | ISP, Wi-Fi | Medium × high | Accept: transport encryption hides content only; DPI resistance is research |

### Spoofing

Class 5 in detail. In place: profiles are signed by the owner's identity key and the Peer ID is
its hash, so a profile cannot be forged, only copied under a new key; node IDs carry proof of
work. The profile has no name field yet, and only `place` is checked for control characters,
which lets bidi overrides and invisible characters through.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| A clone: a new key with someone's photos, fields and name | Search, profile links | High × high | Accept: a contact is shown as unverified until the safety number or QR code is compared; reports are class 14 |
| A clone lists the same photo blob hashes | Profile photos | Medium × high | Accept: an exact-hash check is defeated by re-encoding; the unverified mark is the defence |
| A lookalike name: homoglyphs, invisible characters, bidi overrides | Profile strings | High × high | Fix: an optional public `name`; every profile string is NFC and refuses control, format (bidi, zero-width) and private-use characters, checked by the node on publish and by the client on input and display |
| A public name passes for an identity | Contacts, chats | High × high | Fix: a local name the user gives a contact replaces the public one; a public name is always shown with the verified or unverified mark |
| A Peer ID ground to share a visible prefix or suffix | Contact list, links | High × medium | Fix: the UI never shows part of a Peer ID as identity; contacts are checked only by the full safety number or QR code |
| A search node claims the role and returns profiles that do not match, or hides those that do | Search | Medium × medium | Fix: the client checks the conditions on the signed profiles it gets, drops mismatches and asks 2 search nodes; withholding is class 8 |
| A node claims the TURN or store role it does not serve | DHT providers | Medium × medium | Accept: TURN sees SRTP only and store data is signed; a node that only withholds is class 8 |
| Lookalike seed hosts or domains | Seeds, DNS | High × medium | Out of scope here: seeds carry peer IDs ([above](#mitm-and-key-verification)), domains are class 10 |
| A group member poses as another member | MLS groups | High × medium | Fix: an MLS credential is accepted only for a device listed in the newest profile of the identity it names |
| A forwarded message passes for the original author's words | Chats | Medium × medium | Fix: forwarded content is marked as forwarded and its author is never shown as verified |
| Who made a media blob | Media | Low × medium | Accept: a blob is authentic by its hash in a signed profile or an MLS message |
| A scammer under their own real identity | Chats | High × high | Out of scope here: class 11 |

## Out of scope

- Global passive adversaries correlating traffic across the network.
- Denial of service at the network layer (volumetric floods below libp2p); operators use their
  hosting's protection.
- A compromised operating system on a user's device.
