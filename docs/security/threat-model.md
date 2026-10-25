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
| **Cloud host** (provider staff, a stolen cloud account, compelled access) | Behind an honest operator's back: dump the node VM's memory, read its disk, snapshots and backups, write to its disk or restore an old snapshot ([below](#cloud-host-access)) |
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
| Operator or observer reads messages | MLS end-to-end encryption ([ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md)) | ⚠️ Partial: MLS is implemented in `rust/messaging`; no client sends messages with it yet |
| Forged or rolled-back profile | Ed25519 signature by the owner; a node rejects an older version | ✅ Implemented; a node can still serve an older version it kept, and `rust/client` refuses a version below the one it holds ([below](#data-tampering)) |
| Another client reads or empties a mailbox | Fetch and ack signed by the device key over a nonce issued for this connection | ✅ Implemented ([mailboxes](../architecture/bootstrap.md#mailboxes)) |
| Replayed mailbox request | The nonce is bound to one libp2p connection; other requests are idempotent | ✅ Implemented |
| Spam into a mailbox | Size limits per envelope and mailbox, puts limited per sender key and IP group | ✅ Implemented; the sender key is free, so the IP group is the real bound |
| Request floods | Token buckets per peer ID and IP group, local ban after repeated refusals | ✅ Implemented ([rate limiting](../architecture/bootstrap.md#rate-limiting)) |
| Disk, traffic or connection exhaustion | Store size caps, free-space floor, byte rate, connection, stream and memory limits | ✅ Implemented ([resource guards](../architecture/bootstrap.md#resource-guards)) |
| Oversized or malformed messages | Per-protocol wire caps: 1 MiB + 64 KiB, or 6 MiB + 64 KiB for media, protobuf decoding, unknown variants answered `UNSUPPORTED` | ✅ Implemented |
| Operator withholds or drops data | R = 5 replicas on independent nodes; the client tries another replica ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)) | ⚠️ Partial: nodes replicate and repair ([replication](../architecture/bootstrap.md#replication-and-repair)); client checks are planned ([below](#hostile-node)) |
| Abuser keeps using one node | Operator deny list of peer IDs, IP groups and key hashes | ✅ Implemented ([deny list](../architecture/bootstrap.md#deny-list)); a key is free to regenerate, so IP-group quotas carry the rest |
| Sybil eclipse of a key or a node | Node-ID proof of work, routing-table IP diversity, disjoint lookups, replicas at H(key ‖ i), local reputation ([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)) | ✅ Implemented ([routing defences](../architecture/p2p-networking.md)) |
| No way in without one seed | Several seeds, DNS seed names, a cache of peers seen before | ⚠️ Partial: seeds the operator writes, the node's peer cache and anchors exist; node list distribution is out of scope for the first release ([joining](../architecture/p2p-networking.md#out-of-scope-for-the-first-release)) |
| Search index flooded with fake profiles | Profile proof of work bound to the owner's key ([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)) | Fix: implemented canonical places, newcomer byte quota, largest-place eviction and daily heartbeat proof of work ([search](../architecture/bootstrap.md#search)); query floods face a fair queue and load puzzle ([rate limiting](../architecture/bootstrap.md#rate-limiting)). This bounds fresh-key damage; accepted residuals are listed under [denial of service](#denial-of-service) and quantified in [search capacity](../architecture/search-capacity.md#adversary) |
| Metadata: who talks to whom | Mailbox addresses are device keys, not user IDs; delivery receipts travel as ordinary encrypted messages | ⚠️ Partial: sealed sender hides who writes (a fresh key per envelope); the operator still sees which mailbox gets mail, when, and from which IP ([privacy](privacy.md)) |
| Stolen identity or device key | Keys stay on the device, separate transport keys ([ADR 0006](../decisions/0006-transport-keys-separate-from-identity.md)); MLS post-compromise security | ⚠️ Partial: Android, iOS and macOS wrap the identity key with the platform keystore; MLS is in `rust/messaging`, its state in memory only |
| Operator links peer IDs to IPs | — | Not defended: no relay or proxy is designed |

## Threat classes

Every class, rated impact × likelihood for the project today and ordered by criticality. Each
class gets its own analysis that decides fix or accept per attack; the table above lists the
defences already in place. Nodes are untrusted by design
([ADR 0007](../decisions/0007-open-bootstrap-network.md)).

| # | Class | Impact | Likelihood | Status |
|---|-------|--------|------------|--------|
| 1 | Weak cryptography: primitives, nonces, entropy, key storage | Critical | Medium | ⚠️ Partial: MLS payloads, their sealed state and their mailbox delivery in `rust/client`; SFrame for frames in `rust/video`; the app runtime on top of them, the media path and desktop keystores are planned ([encryption](encryption.md)) |
| 2 | Supply chain: dependencies, build, CI, releases, update channel | Critical | High | ⚠️ Partial: lock files, dependency audit, pinned CI; the age check and signed releases are planned ([below](#supply-chain)) |
| 3 | MITM on end-to-end keys and on joining (seed list, DNS) | Critical | Medium | ⚠️ Partial: transport is authenticated and node seeds can pin peer IDs; client key verification and a pinned built-in seed list are planned ([below](#mitm-and-key-verification)) |
| 4 | Traffic analysis and deanonymization of users | Critical | High | ⚠️ Partial: transport encryption, E2E signals, padding and sealed sender in `rust/client`; relay-only calls are planned ([below](#traffic-analysis), [privacy](privacy.md)) |
| 5 | Spoofing: impersonated users, lookalike names and peer IDs, fake nodes | High | High | ⚠️ Partial: owner signatures, node-ID proof of work, profile text rules; the search result check is implemented as a library function; local names and the client that runs the check are planned ([below](#spoofing)) |
| 6 | Hijack of AI agents and developer tooling (prompt injection) | Critical | Medium | ⚠️ Partial: agent rules, no agents in CI; a narrow token, secret scanning, protected `main` and signed commits are planned ([below](#ai-agents-and-developer-tooling)) |
| 7 | Hostile pull requests | Critical | Medium | ⚠️ Partial: external CI approval, pinned-head review, a PR guard and branch rules; the maintainer bypass and PR-editable workflows remain ([below](#hostile-pull-requests)) |
| 8 | Hostile node: withholding, censorship, search poisoning, harvesting | High | High | ⚠️ Partial: replication, repair, Sybil defences and local reputation on nodes; client checks are planned ([below](#hostile-node)) |
| 9 | Denial of service beyond the current guards: CPU asymmetry, crafted payloads | High | High | ⚠️ Partial: rate limits, resource guards, frame caps, per-group connection limits, a search fair queue and puzzle; write tokens, a TURN quota per IP group and fuzzing are planned ([below](#denial-of-service)) |
| 10 | Homoglyphs and Unicode tricks: hidden code, typosquatting, lookalike domains and apps | High | Medium | ⚠️ Partial: Rust confusable and bidi lints and a repository Unicode check; official names are planned ([below](#homoglyphs-and-unicode-tricks)) |
| 11 | Social engineering and operations: phishing, account takeover, fake releases | High | Medium | ⚠️ Partial: seeds pinned to peer IDs, per-node deny lists; a security policy, account hardening and in-app warnings are planned ([below](#social-engineering-and-operations)) |
| 12 | Privilege escalation and host blast radius of a compromised node | High | Medium | ❌ Planned |
| 13 | Disclosure of node internals: private addresses, versions, logs, metrics | Medium | High | ✅ Implemented: metrics only on an opt-in loopback listener without peer or key labels, no software version in `NodeInfo`, logs without peer identifiers, only global addresses and the agent version `dyapp` in identify ([below](#node-internals-disclosure)) |
| 14 | Illegal content planted on nodes, legal pressure on operators | High | Medium | ⚠️ Partial: deny list, local purge of listed blobs and an operator guide, technical abuse response only (accepted residual risk) ([below](#illegal-content-and-legal-pressure)) |
| 15 | Data tampering: media, deny lists, signature encoding, rollback | High | Medium | ⚠️ Partial: owner signatures over the bytes as received, content addressing, version checks, and a client-side profile rollback check in `rust/client` ([below](#data-tampering)) |
| 16 | Client-side exposure: backups, notifications, logs | High | Medium | ⚠️ Partial: platform data protection and backup exclusion; private notifications, log rules and device revocation are planned ([below](#client-side-exposure)) |
| 17 | Reflection: an honest node sends traffic to a third party | Medium | Medium | ✅ Rated: checked identify and provider addresses, a dial guard, replica caps and TURN deny ranges; the rest accepted ([below](#reflection-through-an-honest-node)) |
| 18 | Injection: SQL, paths, logs, commands, rendering | High | Low | ⚠️ Partial: bound SQL parameters, hash-named files, plain-text rendering; profile string limits planned ([below](#injection)) |
| 19 | Replay | Medium | Low | ✅ Rated: connection-bound nonces, versions, the 10-minute window and refused released attachments; the rest accepted ([below](#replay)) |
| 20 | Downgrade and weak-parameter negotiation | High | Low | ✅ Implemented: nothing is negotiated; announced limits are hints within compiled floors ([below](#downgrade)) |
| 21 | Side channels and timing | Medium | Low | ✅ Rated: no secret comparison on nodes, constant-time rule for secrets; the rest accepted ([below](#side-channels-and-timing)) |
| 22 | Clock manipulation against freshness windows and TTLs | Medium | Low | ⚠️ Partial: 10-minute windows, authenticated time sync on nodes; client clock correction planned ([below](#clock-manipulation)) |
| 23 | Cloud host access to a node VM: memory dump, disk read, disk write | High | Medium | ⚠️ Partial: no client IPs on disk, a key-compromise runbook; no core dumps or swap; the node key sealed to the host and its vTPM; binary replacement, snapshot rollback and planted content are accepted for v1 ([below](#cloud-host-access)) |

### Cryptography

Class 1 in detail. Keys and rules: [Encryption & Security Status](encryption.md#keys).

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Stubs taken for crypto: payload passed through, `sign` returns the message, XOR key derivation, random unchecked frame tags | Messages, video frames | Critical × certain | Fixed: the stubs are deleted; MLS is in `rust/messaging/src/mls.rs`, SFrame in `rust/video/src/sframe.rs` |
| Forged, malleable or weak-key Ed25519 signature | Any `SignedRecord` | Critical × low | Accept: `verify_strict` rejects non-canonical and small-order input; every check goes through `dyapp-identity` |
| A signature of one kind replayed as another | Signed records; the node key signs deny lists and libp2p handshakes | High × low | Accept: one complete registry and tests enforce distinct labels ending in `\0`, prefix freedom, separation from the locked libp2p signing contexts and rejection of cross-domain verification |
| One key serving two purposes | Identity, device, MLS and libp2p keys | High × medium | Fix: one key per purpose, listed in [keys](encryption.md#keys) |
| MLS ciphersuite downgrade or unknown suite | KeyPackages and Welcomes from untrusted nodes | Critical × medium | Fix: only 0x0001 is accepted, nothing is negotiated |
| A group member whose credential does not chain to the Peer ID | MLS Add, Welcome | Critical × medium | Fix: checked on every Add and Welcome; verifying the Peer ID itself is class 3 |
| Frame nonce reuse under one key | Video frames | Critical × high with random nonces | Fixed in `rust/video/src/sframe.rs`: SFrame ([RFC 9605](https://www.rfc-editor.org/rfc/rfc9605)), a counter nonce per sender key, keys from the MLS exporter |
| Attachment key or nonce reuse, a guessable release secret | Media blobs | High × medium | Fix: a fresh random key per attachment, counter nonces, a 32-byte release secret |
| Weak or failed randomness | Key generation, nonces, secrets | Critical × low | Accept: only the OS generator (`getrandom`), which blocks until seeded and fails closed; a cloned VM relies on the kernel's VM generation ID (Linux 6.1 on Debian 12) |
| Identity or device key read from disk or backups | Client storage | Critical × medium | Fix: wrapped by the platform keystore, excluded from backups and cloud sync |
| Secret copies left in memory | `Identity::secret`, key generation, key file buffers | Medium × low | ⚠️ Partial: owned node-key serialization/loading and identity generation/export buffers use `Zeroizing` and are cleared on drop; caller copies and platform key wrapping remain separate work. Compiler/library/OS copies are not covered; swap and core dumps of a compromised OS are out of scope |
| Node key read from disk or backups | `node.key` | Medium × medium | Accept: sealed by `systemd-creds` in the package ([cloud host](#cloud-host-access)), else created with mode 0600; startup checks the opened file and refuses group/other permission bits without changing them; backups made with `umask 077`. A copied key is not revocable: the operator wipes the node and runs keygen for a new peer ID, and others replace or deny the old one ([runbook](../manuals/bootstrap.md#node-key-compromise)) |
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
| A node hands one ordinary KeyPackage to several senders | KeyPackage fetch | Medium × medium | Mitigation: a responding replica durably consumes an ordinary package before reply; another replica can still allocate its copy. The recipient rejects replayed Welcomes and a sender retries; last-resort reuse weakens first-message forward secrecy ([ADR 0019](../decisions/0019-device-directory-and-keypackage-rendezvous.md)) |
| A stolen identity key adds an attacker's device to the profile | Profile publish | Critical × low | Fix: every chat with the contact shows "new device"; a verified contact loses its verified mark until checked again |
| First contact through a node, not out of band | Search, forwarded contact | Critical × medium | Fix: the safety number or QR code is derived from both Peer IDs; until it is compared the contact is shown as unverified |
| A node in the middle of linking a new device | Device linking | Critical × medium | Fix: keys and history travel only over a channel keyed by a secret in the QR code; nodes carry ciphertext only |
| Swapped DTLS fingerprints in call signaling, a TURN relay in the middle | Video signaling, TURN | Critical × medium | Fix: offer and answer travel only as MLS messages; TURN sees SRTP only ([Video](../architecture/video.md)) |
| Another node answers on a peer's address | libp2p dial | High × low | Accept: TLS and Noise bind the connection to the libp2p key; a dial with a peer ID refuses any other key |
| A hijacked DNS name, domain or seed host: a new node or client sees only the attacker's nodes (eclipse) | Seeds, DNS | Critical × medium | ⚠️ Partial: node seeds accept `host:port/<peer ID>` and refuse another key; plain seeds remain unpinned. A pinned built-in client list is out of scope for the first release; asking several seeds and cross-checking disjoint lookups are planned |
| A free dynamic DNS name for a seed is lost, reassigned, or repointed with a stolen update token | Seeds, DNS provider | High × medium | Mitigation: the [manual](../manuals/bootstrap.md#without-a-domain) says to pin the peer ID, keep the token private and list seeds at more than one provider; a pinned seed then fails closed |
| A DNS provider outage or a stale answer hides a seed | Seeds, DNS provider | Medium × medium | Accept: several seeds and the peer cache cover it; a stale address is a failed dial, and a pin refuses a wrong peer |
| A fake node answers every protocol | Client to node | High × medium | Accept: it cannot forge signed or MLS data, only withhold it, which is class 8 |
| Lookalike Peer IDs, names or domains | Contact list, links | High × medium | Out of scope here: classes 5 and 10 |

### Supply chain

Class 2 in detail. In place: `Cargo.lock` with `--locked` builds, `cargo deny` (crates.io only, no
git sources, yanked crates refused, OpenSSL banned), `cargo audit` weekly and on every change,
GitHub Actions and Docker build images pinned by digest, downloaded tools, the Gradle wrapper and Gradle artifacts
checked by SHA-256, and an SPDX SBOM per CI run.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| A malicious or hijacked crate version, a typosquat, a `build.rs` or proc macro that runs at build time | Cargo dependencies | Critical × medium | In place: no crates.io version younger than 14 days, enforced by `make quality` ([dependency age](../development/code-quality.md#dependency-age)) and a 14-day Dependabot cooldown; a new dependency is reviewed by hand |
| A malicious Gradle artifact | Android dependencies | Critical × medium | In place: Gradle dependency verification, SHA-256 for every artifact ([build](../development/build.md#gradle-dependency-verification)) |
| An unpinned tool pulled in CI: `cargo install cargo-deny` in CodeQL, `pip install flake8`, xcodegen without a checksum, `dtolnay/rust-toolchain` at a branch, `debian:bookworm-slim` by tag | CI workflows, test Dockerfiles | High × medium | Fix: pinned versions with `--locked`, checksums and digests |
| A pull request poisons the GitHub Actions build cache read by `main` | Docker builds with the GHA cache | High × medium | Fix: only pushes to `main` write the cache, pull requests read it; every workflow sets `permissions` at the top |
| A tampered Debian package from the mirror | apt in build images | Medium × low | Accept: Debian repositories are signed; versions are not pinned |
| A forged or tampered release: deb, APK, app bundle, container image | Downloads, image registry | Critical × medium | Partial: a `vX.Y.Z` tag publishes the `dyappd` package, tarball, SBOM and GHCR image with Sigstore-signed SLSA build provenance ([ADR 0020](../decisions/0020-dyappd-release-channels.md)); nothing is published yet, the client artifacts have no pipeline, and the [operator manual](../manuals/bootstrap.md#updates) does not yet show the `gh attestation verify` check. Until then an operator builds from a tagged commit |
| A node's `node.key` is copied from a host or a backup | Node host, backups | High × low | Accept: a key cannot be revoked; the operator starts the node over with a new key and peer ID ([node key compromise](../manuals/bootstrap.md#node-key-compromise)) |
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
| An operator maps the envelope's sender key and the mailbox to two public profiles: the social graph | Stored envelopes | Critical × high | Fixed in `rust/client` (`Node::put`): sealed sender. Each envelope is signed by a fresh one-off key; the sender's identity is only inside the MLS ciphertext. Per-key quotas then bound nothing; the IP-group and mailbox quotas remain |
| An operator learns whose mailbox an envelope is for | Mailbox address | Medium × high | Accept: the recipient is known, as in Signal; the mailbox hash is derived from the public device list |
| An operator tells signals from messages by `Envelope.class` | Stored envelopes, replicas | Low × high | Accept: the node needs the class for the separate quota; a like and a view stay indistinguishable |
| Envelope size tells a like or view from text or a photo message | Stored envelopes, network | Medium × high | Fix: the plaintext is padded to the next power of two from 1 KiB before encryption (`rust/client`, `chat::pad`) |
| Every replica links one contact's envelopes to a mailbox within a day by their shared write token key | Stored envelopes, replicas | Low × high | Accept: the key is a daily pseudonym issued inside MLS, not an identity; it shows how many messages one contact sent that day, kept on disk until the TTL |
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
work. The profile has no name field yet; every profile string is NFC and refuses control, format
(bidi, zero-width) and private-use characters except ZWNJ and ZWJ
([ADR 0026](../decisions/0026-profile-text-rules.md)).

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| A clone: a new key with someone's photos, fields and name | Search, profile links | High × high | Accept: a contact is shown as unverified until the safety number or QR code is compared; reports are class 14 |
| A preview that does not match the original (bait) | Chat previews, profile thumbnails | Medium × medium | Accept: no worse than sending the image itself; a stranger's original downloads only on a tap |
| A clone lists the same photo blob hashes | Profile photos | Medium × high | Accept: an exact-hash check is defeated by re-encoding; the unverified mark is the defence |
| A lookalike name: homoglyphs, invisible characters, bidi overrides | Profile strings | High × high | In place: every profile string is NFC and refuses control, format (bidi, zero-width, except ZWNJ and ZWJ) and private-use characters, checked by `verify` on node publish and client display, and `is_clean_text` for client input. Fix: an optional public `name` under the same rule; homoglyphs stay, the unverified mark is the defence |
| A public name passes for an identity | Contacts, chats | High × high | Fix: a local name the user gives a contact replaces the public one; a public name is always shown with the verified or unverified mark |
| A Peer ID ground to share a visible prefix or suffix | Contact list, links | High × medium | Fix: the UI never shows part of a Peer ID as identity; contacts are checked only by the full safety number or QR code |
| A search node claims the role and returns profiles that do not match, or hides those that do | Search | Medium × medium | Fix: the client checks the conditions on the signed profiles it gets, drops mismatches and older versions and asks 2 search nodes ([results](../architecture/bootstrap.md#search); the check is implemented, the client calling it is planned); withholding is class 8 |
| A node claims the TURN or store role it does not serve | DHT providers | Medium × medium | Accept: TURN sees SRTP only and store data is signed; a node that only withholds is class 8 |
| A flood of DHT records or providers under many keys fills a node's Kademlia store | DHT | Medium × medium | Fix: no remote value records are stored; a remote provider only for the search and TURN keys, from a peer ID with the node-ID proof of work, on the IP of a live connection |
| Sybils with ground node IDs take the 20 provider slots of the search or TURN key | DHT providers | Medium × low | Accept: each needs the node-ID proof of work and a live connection within the IP-group connection limits; a wrong relay or search node is the row above |
| Lookalike seed hosts or domains | Seeds, DNS | High × medium | Out of scope here: seeds carry peer IDs ([above](#mitm-and-key-verification)), domains are class 10 |
| A group member poses as another member | MLS groups | High × medium | Fix: an MLS credential is accepted only for a device listed in the newest profile of the identity it names |
| A forwarded message passes for the original author's words | Chats | Medium × medium | Fix: forwarded content is marked as forwarded and its author is never shown as verified |
| Who made a media blob | Media | Low × medium | Accept: a blob is authentic by its hash in a signed profile or an MLS message |
| A scammer under their own real identity | Chats | High × high | Out of scope here: class 11 |

### AI agents and developer tooling

Class 6 in detail. In place: AI agents run only in the maintainer's sessions on the developer
host, never in CI; they commit and push only when the maintainer allows it, and their commits
carry a `Co-Authored-By` trailer. They run with the maintainer's git, GitHub and beads access.
The maintainer can bypass the `main` PR ruleset for direct pushes until the first public release.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Prompt injection in issues, pull requests, review comments, web pages, crate READMEs or dependency sources | Text an agent reads | Critical × medium | Fix: the agent rules treat such text as data, never as instructions, and tell the agent to report text that reads like instructions |
| A hijacked agent pushes harmful code to `main` | git push | Critical × medium | Accept until the first public release: the maintainer account can bypass the PR ruleset. Before release remove that bypass and require signed commits |
| An agent leaks host secrets through a command or an outbound request | Developer host | Critical × medium | Fix: agent sessions use a fine-grained GitHub token for this repository only. Accept: the rest of the host environment stays readable |
| A hostile or compromised MCP server, plugin or hook | Developer host | Critical × low | Accept: the maintainer picks and updates these tools |
| Instructions planted in beads | `.beads`, the Dolt remote | High × low | Accept: only the maintainer writes beads; their content is data for the agent |
| An AI agent in CI runs on a hostile pull request | Workflows | Critical × low | Fix: no AI agent runs in CI; if one is added, it gets no secrets, a read-only token and never runs on `pull_request_target` |
| A secret is committed | git | High × medium | Fix: GitHub secret scanning and push protection |
| A hostile pull request by a person | Pull requests | Critical × medium | Out of scope here: class 7 |
| A taken-over maintainer account | GitHub account | Critical × low | Out of scope here: class 11 |

### Hostile pull requests

Class 7 in detail. The [review policy](../development/pr-review.md) and
[ADR 0021](../decisions/0021-ai-review-human-merge.md) require the maintainer to read an external
PR before approving CI, two independent local reviews
at the same head SHA, green required checks and a head-pinned merge. The repository ruleset
requires PRs and checks, rejects force-pushes and deletion, and temporarily lets the maintainer
bypass it. GitHub Actions requires approval for every external contributor's fork PR. These
settings were applied through the GitHub API and read back on 2026-10-23.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| A hostile PR hides code in renames, generated files or a large diff | Review | Critical × medium | Fix: the checklist covers these cases; two independent reviewers report findings on a pinned SHA, and large changes are split |
| A PR changes CI or build scripts to run privileged code | Workflows | Critical × medium | Partial: external CI needs maintainer approval after an infra diff review; the existing Android CI uses `sudo`, so PR code can run with runner privileges once approved, but receives no repository secrets |
| A PR introduces a malicious dependency or alternate registry | Cargo.lock | Critical × medium | Fix: `pr-guard` reports changed packages, rejects non-crates.io sources and versions under 14 days old; human review covers package identity and build scripts |
| A PR alters the guard or its workflow | `pull_request` workflow | Critical × medium | Accept: the PR can edit its own workflow; a warning is shown, while the trusted-base checker and maintainer's pre-run review remain the boundary |
| A PR tries to poison the build cache read by `main` | Docker BuildKit | High × medium | In place: PRs read the GHA cache but `docker-local.py` omits `--cache-to` on PRs; `main` writes it |
| A stolen maintainer session directly pushes malicious code | Ruleset bypass | Critical × medium | Accept until release; remove the bypass and require signed commits before publishing |

### Hostile node

Class 8 in detail. In place on nodes: replicas at R = 5 points with repair
([replication](../architecture/bootstrap.md#replication-and-repair)), node-ID proof of work,
disjoint lookups and IP-group limits ([routing defences](../architecture/p2p-networking.md)),
local reputation and storage trust, and the profile proof of work. Every record is signed or
encrypted end to end, so a node can withhold or delay data but not forge it. In `rust/client`,
clients score the nodes they use and cross-check their mailbox at a second replica.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| The accepting node drops an envelope or claims two holders it does not have | `mailbox.put` | High × high | ⚠️ Partial: `Session::receive` answers with a delivery receipt inside MLS, and without it the sender's outbox resends 1 h after acceptance, then up to a day apart, for 30 days; a resend tries another of the caller's nodes first, and a receipt for a later write lowers the score of the node that accepted the earlier one. Partial: scores live in memory only, and the app does not pass one node per replica key yet |
| A node hides part of a mailbox from its owner | `mailbox.fetch` | High × high | In place in `rust/client`: `chat::cross_check` fetches from one node and from a node of another replica key on an MLS gap and once a day, and an envelope only the second holds counts against the first ([messaging](../architecture/messaging.md#mailbox-delivery--planned)). Fix: the app calling it on that schedule is planned with the client store |
| A node refuses one user | put, fetch, get | High × medium | Fix: the client moves to another replica key ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)) |
| A node serves an older signed profile | `profile.get` | High × medium | Fix: the client keeps the newest version seen and asks several replicas ([above](#mitm-and-key-verification)) |
| A search node poisons, reorders or hides results | Search | Medium × high | Fix: the client checks results and asks 2 nodes ([above](#spoofing)) and ranks results itself. Accept: a node still picks which matches it returns |
| An accepting node sets false `source_group` labels to steer tokenless eviction | `replica_put`, repair | Low × medium | Accept: replicas file every label under the acceptor's own `via`, so it can only evict envelopes it handed in ([tokenless pool](../architecture/bootstrap.md#mailbox-envelope)) |
| A node lies in inventory during repair | Node to node | Medium × medium | Accept: every copy is verified on its own, and only holders of a replica key can pull its envelopes |
| A holder shortens the expiry or ages the owner of a copy it repairs | `replica_put`, repair | Low × low | Accept: only copies the receiver lacks, never past its own TTL, and the other replicas keep the right expiry |
| A forged node-ID or profile proof of work | DHT, search | High × medium | Accept: both are checked; such a node is not routed to, such a profile is not indexed |
| False roles or false limits in `info` and status | DHT, `/dyapp/node` | Medium × medium | Accept: the client treats `info` as a hint within compiled floors ([Downgrade](#downgrade)); a role that is not served costs the node reputation |
| Harvesting peer and profile data | DHT, search | Medium × high | Accept: profiles are public by design and Kademlia routing tables are open; metadata is class 4 ([above](#traffic-analysis)) |
| Colluding or Sybil nodes take every replica of a key | DHT | High × medium | Accept: [ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md) makes a targeted eclipse costly, not impossible |
| A holder that is no longer closest keeps its copy unseen | DHT | Medium × medium | Fix: holder-side republish ([network growth](../architecture/bootstrap.md#replication-and-repair)) |
| Sybils near a key receive republished data | DHT, republish | Medium × medium | Accept: republish goes only to trusted peers, the data is encrypted or signed, old holders keep their copies, and the exposure lasts at most one TTL |
| Churn causes republish traffic | DHT, republish | Low × medium | Fix: a key is handed over at most once an hour, 16 at a time, and only while traffic is under 50 % |
| A client keeps using nodes that failed it | Client | Medium × high | In place: `rust/client` `scores` counts refusals, timeouts, invalid data and older profiles per node, halves the penalty daily and tries the lowest penalty first, falling back across replica keys ([replication](../architecture/bootstrap.md#replication-and-repair)); missing receipts count once delivery receipts exist |

### Denial of service

Class 9 in detail. In place: token buckets per peer ID and IP group with local bans
([rate limiting](../architecture/bootstrap.md#rate-limiting)), disk, traffic, memory, connection
and stream guards, frame size caps and request timeouts
([resource guards](../architecture/bootstrap.md#resource-guards)), and Argon2id checks on their
own two permits with a cache of confirmed proofs ([search](../architecture/bootstrap.md#search)).
Volumetric floods below libp2p stay out of scope ([below](#out-of-scope)).

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| A stranger fills a mailbox to its 10 MiB with junk, so the owner gets `FULL` and no messages | `mailbox.put` | High × high | Fix: write tokens. The owner gives each contact, inside MLS, a write token signed by the mailbox key; envelopes with a token use the 10 MiB, envelopes without one (a first message) go to a small pool per mailbox evicted by peer and source group. The node sees a token pseudonym, not the sender. Implemented: `limits.require_write_tokens` is on by default, client sessions trade tokens after a join, with signed revocation of a blocked contact's token keys ([tokenless pool](../architecture/bootstrap.md#mailbox-envelope)) |
| Fresh keys flood a store with envelopes or profiles, so it evicts the oldest real records | `mailbox.put`, `profile.publish` | High × medium | Fix: the tokenless pool and a store-wide tokenless share (`limits.tokenless_share_pct`, implemented) bound envelopes; store nodes check the profile proof of work as search nodes do |
| A botnet across many IP prefixes and peers fills a mailbox's tokenless pool, so real first messages are evicted | `mailbox.put` | Medium × low | Accept: the pool is best effort; contacts with a write token are not affected, and the sender of a first message retries |
| Many peer IDs from one IP group open thousands of connections; the per-peer limit is 4 | libp2p | Medium × high | Implemented: `max_connections_per_ip_group` (default 32) combines pending and established incoming connections; outgoing exempt. Failure/close frees slots; real TCP tests cover the 33rd refusal and composed guard cleanup ([resource guards](../architecture/bootstrap.md#resource-guards)) |
| An attacker rotates /48 prefixes inside its /32, or IPv4 addresses inside a /16 | All requests | Medium × medium | Implemented: a second, coarse bucket per /16 and /32 with ten times the group budget ([rate limiting](../architecture/bootstrap.md#rate-limiting)). Accept: users behind one CGNAT share a bucket |
| Crafted protobuf, SDP or MLS input panics a decoder or allocates without bound | Every codec | High × medium | Fix: cargo-fuzz targets for every network codec, run in CI |
| Relay traffic through TURN | `node.turn`, coturn | Medium × medium | Fix: the coturn example sets `user-quota`, `total-quota` and `max-bps` |
| Bypass the TURN `user-quota` with fresh peer IDs: the credential username carries a free peer ID | `node.turn`, coturn | Medium × high | Fix: the username carries a keyed pseudonym of the client's IP group instead, so one group shares one quota ([TURN credentials](../architecture/bootstrap.md#turn-credentials)); `total-quota` and `max-bps` bound the relay as a whole |
| Slow SQLite calls stall the swarm loop, so cheap requests starve every peer | Store requests | Medium × medium | Implemented: store requests run on blocking workers off the swarm loop, after the rate limits ([served protocol](../architecture/bootstrap.md#served-protocol)) |
| Expensive search queries, fake profile floods, signal spam, catch-up traffic, messages to non-matches | Search, mailbox | Medium × high | Implemented for search queries: a fair queue per IP group and a load-driven hash puzzle ([search](../architecture/bootstrap.md#search)). Implemented against fake profiles: canonical places, a newcomer quota, eviction by place and heartbeat proof of work; accepted residual risks in [search capacity](../architecture/search-capacity.md#adversary). Implemented against signal spam: signals have their own per-mailbox quota with the oldest evicted, so they never block messages ([limits](../architecture/bootstrap.md#limits-and-abuse)). Catch-up traffic and the non-match limit are planned separately |
| Fakes remain alive through the 30-day newcomer window | Search publication and heartbeat | Medium × high | Accept: paying daily proof of work makes them established; the newcomer quota no longer bounds their total share. Largest-place eviction still applies ([capacity model](../architecture/search-capacity.md#adversary)) |
| Fakes claim a real rare place | Search publication | Medium × high | Accept: canonical IDs prevent invented places, not a false claim about a real one; water-filling cannot distinguish genuine residents from fakes |
| Honest newcomers compete with fresh fakes | Search publication | Medium × high | Accept: both share the 10 % record-byte quota and oldest-heartbeat eviction. Size eviction targets established keys only, so it does not repeatedly halve the newcomer pool; quota pressure and TTL can still remove honest newcomers before 30 days ([search](../architecture/bootstrap.md#search)) |
| A malicious peer seeds fakes during a new node\'s first 30 days | Inventory catch-up | Medium × medium | Accept: catch-up keys enter as established during this grace window to let a new node bootstrap; the newcomer quota cannot protect this path |
| GPU or many IP groups solve the load puzzle cheaply | Search query/get | Medium × high | Accept: even 20 bits is not a throughput bound on fast hardware; round-robin limits one active IP group to one fair share, while a distributed attacker can acquire many shares ([capacity model](../architecture/search-capacity.md#adversary)) |
| A flood of profile changes during a search index rebuild keeps it from settling | Search publication and heartbeat | Low × medium | Accept: after three postpones or overflows in a row the node stops writes for one full replay under the writer lock (at most 262 144 tracked keys) and logs `stop_the_world` ([rebuild](../architecture/bootstrap.md#storage-on-a-node)) |
| Filling the disk during a search index rebuild | Any write | Low × low | Accept: after three free-space aborts in a row the node drops the index and refills it by catch-up, answering `partial` meanwhile, and logs `drop_refill`; clients ask other search nodes |
| Signature and Argon2id checks used to burn CPU | Writes, search | Low × medium | Accept: Ed25519 is cheap and runs after the buckets; Argon2id has its own permits and a cache |
| Slow or idle streams hold connections | libp2p | Low × medium | Accept: requests time out after 20 s, idle connections after 60 s |

### Homoglyphs and Unicode tricks

Class 10 in detail. In place: rustc refuses bidi control characters in Rust code, and
`clippy -D warnings` turns `confusable_idents` and `mixed_script_confusables` into errors.
`check-repository.py` refuses format and private-use characters in every tracked file, non-ASCII
paths and non-ASCII Swift and Kotlin sources
([code quality](../development/code-quality.md#hidden-unicode)). Lookalike names in profiles are
class 5 ([above](#spoofing)).

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Trojan Source: bidi overrides or invisible characters in Dart, Kotlin, Swift, Python, shell, YAML or docs | Pull requests, dependencies | High × medium | In place: `make quality` and CI refuse Unicode format (bidi, zero-width) and private-use characters in every tracked text file |
| Confusable identifiers in Rust | Pull requests | High × low | Accept: refused by the Rust lints above |
| Confusable identifiers in other languages | Pull requests | Medium × low | In place: the same check keeps Swift and Kotlin sources ASCII only; there are no Dart sources yet. Accept: Cyrillic and emoji stay allowed in strings and docs |
| Lookalike file names and paths (Cyrillic letters, invisible characters) | Pull requests | Medium × medium | In place: the same check refuses non-ASCII paths |
| Typosquatted crates, pub or Gradle packages | Dependencies | High × medium | Out of scope here: class 2 ([above](#supply-chain)) |
| A lookalike commit author | Git history | Medium × low | Fix: signed commits on a protected `main` ([above](#ai-agents-and-developer-tooling)) |
| A lookalike repository, organisation, domain or store listing ships a fake app | GitHub, DNS, app stores | High × medium | Fix: one page lists the official names (repository, application ID, signing key fingerprints, domain); before the first public release the domain, its obvious lookalikes, the store IDs and the crate and pub names are reserved; releases are signed (class 2) |
| Lookalike links in docs and issues | Markdown | Low × low | Accept: the format-character check and review |

### Social engineering and operations

Class 11 in detail. In place: a seed may be pinned to its peer ID and the client's built-in list
always is, and that list stays empty until independent operators run public seeds
([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)); an operator's deny list refuses
users on that node only, and lists received from other nodes are stored, never applied. There is
no security policy and no private channel for vulnerability reports.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Phishing or takeover of a maintainer account (GitHub, crates.io, pub, app stores, domain registrar) | Email, fake login pages, SIM swap, account recovery | High × medium | Fix: hardware-key 2FA on every account, recovery codes kept offline, tokens scoped and with an expiry; signed commits on a protected `main` ([above](#ai-agents-and-developer-tooling)) |
| A fake release or a fake download page | GitHub, lookalike domains, stores | High × medium | Fix: signed builds for every platform (class 2) and the official names page with key fingerprints ([above](#homoglyphs-and-unicode-tricks)) |
| A fake "official" seed or node list | Websites, chats | Medium × medium | Accept: seeds are pinned to peer IDs and the built-in list is empty; once public seeds exist, the official names page lists them |
| Fake support channels ask users for keys or codes | Social networks, chats | Medium × medium | Fix: the official names page lists the support channels and says that the project never asks for keys, codes or backups |
| Forged abuse reports push an operator to deny-list a user | Email to operators | Medium × medium | Accept: one node's refusal; the user goes to other nodes. In place: the [operator page](../operations/abuse-and-legal-requests.md#checking-a-report-before-acting) tells operators to check a report before acting and never to apply received lists unread |
| One operator's deny list censors users on other nodes | Deny-list exchange | Medium × medium | Accept: each operator denies on their own node only ([access control](../manuals/bootstrap.md#deny-list)); exchange is off in the first release and received lists are never applied automatically ([exchange](../architecture/bootstrap.md#deny-list-exchange)) |
| A user wrongly denied has no one to appeal to | Operators | Medium × medium | Accept: abuse response is technical only, see [below](#illegal-content-and-legal-pressure) |
| Someone posing as a maintainer asks an operator for node keys or logs | Email, chats | High × low | Fix: the security policy says that maintainers never ask for keys or logs; the deployment guide links it |
| A vulnerability is published before a fix | Public issues | High × medium | Fix: a security policy with GitHub private vulnerability reporting |
| A scammer in a chat asks for a code or sends a lookalike link | Messages | Medium × medium | Fix: messages from people who are not contacts are marked and land in a separate requests list; links show their full host, IDN hosts in punycode. Verification of contacts is class 5 ([above](#spoofing)) |
| Signing keys, the domain and the accounts are held by one person | Maintainer loss | Medium × low | Accept: the code is open and can be forked; a fork gets new names and keys |

### Client-side exposure

Class 16 in detail. In place: app data is excluded from OS backups and device transfer, and the
Android app exports only its launcher activity (the manifest is checked in CI). Keys are wrapped
by the platform keystore, device-only and outside backups ([encryption](encryption.md)); the local
database and MLS state rely on platform data protection.
A compromised OS, root and accessibility malware are out of scope.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Android auto-backup or iCloud copies the database and MLS state to the cloud | OS backups | High × high | In place: `allowBackup="false"` and data extraction rules that exclude everything from cloud backup and device transfer; iOS and macOS mark the app folder as excluded from backup; keys are device-only |
| Lock-screen notifications show the sender and the text | Notifications | Medium × high | Fix: by default a notification says only "New message"; a setting shows the name or the text. Android private visibility, iOS hidden previews |
| Screenshots or the recent-apps thumbnail show chats | OS screenshots, app switcher | Medium × medium | Fix: the app-switcher thumbnail is always blurred; a setting, off by default, blocks screenshots (`FLAG_SECURE`) |
| Other apps read copied keys or safety numbers | Clipboard | Medium × low | Fix: keys and safety numbers are copied marked as sensitive and cleared after 60 seconds. Accept: ordinary copied text |
| Logs or crash reports carry content, keys or contacts | Logs, crash reporters | High × medium | Fix: client logs never carry message content, keys or contacts' identifiers; release builds log warnings and errors only; no third-party crash reporting |
| The local database is read from a device without disk encryption | Desktop files | High × low | Accept: platform data protection on phones, disk encryption on desktops; flash may keep deleted data |
| Another app with ordinary permissions reads app files or calls its components | Android, desktop | High × low | In place: no exported component but the launcher activity (checked in CI); files readable by other apps are impossible from API 24 and the app needs API 31. Accept on desktop: other processes of the same user, keys stay in Secret Service or DPAPI |
| A lost or sold device keeps access to the account | Device loss | High × medium | Fix: another device of the owner revokes it: it leaves the signed device list, contacts remove it from MLS groups (`Group::remove_revoked`) and an identity-signed `revoke` closes its mailbox on the nodes (node and `rust/client` flow implemented; the linking screen and the wipe in the apps are planned). Accept: history stored on a stolen unlocked device; app lock (planned) narrows it |
| A compromised OS reads everything | Root, malware | High × low | Out of scope |

### Node internals disclosure

Class 13 in detail. In place: `NodeInfo` carries roles, limits and retention but no software
version; nodes have no HTTP admin endpoint, and the opt-in metrics listener binds loopback only,
serves only `GET /metrics` and labels series from fixed tables ([metrics
listener](../architecture/bootstrap.md#metrics-listener)); `node status` and `node state` log lines
carry no peer or key identifiers and request logs are not written; `dyappd status` is a local
command. Identify sends the agent version `dyapp` and only the global listen addresses, or all of
them when none is global (a LAN or test network), so a node listening on `[::]` or `0.0.0.0`
keeps its LAN, Docker and VPN addresses.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Learn a node's private LAN, Docker or VPN addresses | Identify listen addresses | Medium × high | Fix, implemented: a node announces only global addresses unless all its listen addresses are non-global (a LAN or test network); receivers drop non-global addresses learned from identify of a peer with a global address |
| Pick an exploit by the libp2p version | Identify agent version | Medium × medium | Fix, implemented: the agent version is the constant `dyapp`, no version. Accept: protocol versions stay visible, they are needed to talk |
| Learn a node's roles from its protocol list and `NodeInfo` | Identify protocols, `node.info` | Low × high | Accept: clients need roles and limits to choose nodes |
| Map the network from Kademlia replies | `FIND_NODE` | Low × high | Accept: routing needs it; Sybil limits are in [Hostile node](#hostile-node) |
| Probe internals through error replies | Request errors | Low × medium | Accept: replies carry status codes only, no paths or internal messages |
| Read logs or status output | Node host | Medium × low | Accept: local to the operator; logs carry no peer or key identifiers. Malformed TOML and config type diagnostics avoid echoing source values that may contain secrets |
| Scrape the metrics endpoint | Metrics | Medium × medium | Fix: implemented; the listener is off by default, binds only `[::1]` and `127.0.0.1`, has no authentication beyond that, bounds connections, head size and time, and carries no peer or key identifiers ([metrics listener](../architecture/bootstrap.md#metrics-listener)). Accept: any local user of the host can read the counters |
| An attack or a failing node goes unnoticed | Operator | Medium × high | Fix: Prometheus metrics on an opt-in loopback listener ([metrics](../architecture/bootstrap.md#metric-families)); alerts are the operator's scraper's job ([monitoring](../manuals/bootstrap.md#status-and-metrics)) |
| Fingerprint a node by response timing | Any request | Low × low | See [Side channels and timing](#side-channels-and-timing) |

### Side channels and timing

Class 21 in detail. In place: nodes compare no secrets. A mailbox nonce is public to the
connection that asked for it, single-use, and checked after the signature; a media release
secret is hashed and looked up by the hash; TURN passwords are checked by coturn. Signature, PoW
and MLS operations come from libraries that keep secret-dependent work constant-time. Message
sizes are covered in [Traffic analysis](#traffic-analysis).

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Recover a secret from comparison timing | Rust and Dart code | High × low | Fix: secrets are compared in constant time ([code quality](../development/code-quality.md#conventions-not-enforced-by-tooling)); no node code compares one today |
| Guess a mailbox nonce from `==` timing | `mailbox` fetch and ack | Low × low | Accept: the nonce is not secret, is single-use and bound to the connection |
| Time signature or PoW checks | Any signed request | Low × low | Accept: the inputs are public; the PoW cache only tells that a public profile was checked recently |
| Learn whether a profile or blob exists from NOT_FOUND | `profile.get`, `media.get` | Low × high | Accept: profiles are public; attachment blobs are addressed by the hash of their ciphertext |
| Learn whether someone's mailbox holds mail | `mailbox` fetch | Medium × low | Accept: a fetch needs the owner's signature; anyone else gets DENIED |
| Tell when a user is online from response delays | Mailbox, presence | Medium × low | Accept: presence goes only to allowed peers; the replica holding the connection is a residual in [Traffic analysis](#traffic-analysis) |
| See a neighbour's activity behind the same NAT from THROTTLED answers | IP-group quotas | Low × low | Accept |
| Read activity from a phone's power use or network pattern | OS, carrier | Low × low | Out of scope: a compromised OS; the network observer is in [Traffic analysis](#traffic-analysis) |

### Illegal content and legal pressure

Class 14 in detail. In place: a per-node deny list of peer IDs, IP groups, key hashes and blob
hashes; received lists are only stored and applied by hand; envelopes are to be end-to-end
encrypted, so an operator can act on a named hash or key, never on content it cannot read.
Nothing scans content.

Abuse response is technical only: the deny list, local purge of listed blobs, rate limits
and the operator runbooks ([access control](../manuals/bootstrap.md#deny-list),
[abuse reports and legal requests](../operations/abuse-and-legal-requests.md)). The
project has no legal procedure, no abuse desk and no appeal, for its own nodes or for third-party
operators; each operator answers for their own node. The maintainer accepted this residual risk
for the first release: a wrongly denied user moves to other nodes, and a complaint the operator
cannot verify technically has no channel in the project ([out of scope](#out-of-scope)).

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Illegal media put on a chosen node to expose its operator (a blob is ground until its hash lands there) | `/dyapp/media` `put`, `shard_put` | High × medium | Fix: listing a blob hash deletes the stored copy, its manifest and its shards at once, refuses `shard_put`, `shard_get` and replica puts of it, and repair never restores it. Accept: the node holds it until the operator lists it |
| Replication spreads a listed blob to honest nodes | Replica puts, repair | Medium × medium | Fix: the same purge on each node that lists it. Accept: each operator lists it on their own; received lists stay manual, so one node cannot censor through another |
| Illegal photo in a public profile | Profile media | High × medium | Accept: the operator lists the blob hash or the owner's key |
| A lawful-access or takedown request asks what the node holds | Operator | High × medium | In place: the [operator page](../operations/abuse-and-legal-requests.md) says what the node stores and for how long, what it never has (IP logs, readable envelopes), how to take a blob or a key down and how to answer a request |
| False copyright or abuse complaints used to censor a user | Operator | Medium × medium | Accept: one node's list removes no one; the client moves to another replica after `REFUSED` |
| Content scanning demanded on the client or the node | Law, operator | High × low | Out of scope: it breaks end-to-end encryption; an operator acts on a named hash or key only |
| Seeds and nodes in one jurisdiction, all pressured at once | Seeds, hosting | High × medium | Accept: the network starts with two seeds and a node's location cannot be checked, so there is no jurisdiction rule; spreading seeds over distant address ranges is a recommendation, not a guarantee |

### Data tampering

Class 15 in detail. In place: every signed record is verified with `verify_strict` over the bytes as
received, under a versioned domain label, and unknown fields stay inside the signed bytes; media
is addressed by its SHA-256 and a rebuilt shard must match it; the highest profile version wins,
a signed tombstone blocks older versions, and the keep list is versioned.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| A node swaps a media blob or a shard | Media fetch, shard rebuild | High × medium | Accept: the key is the SHA-256 of the blob and a rebuilt shard is checked against it; media is encrypted with a key sent inside MLS (planned) |
| A node re-encodes a record, adds or drops fields, or mauls the signature | Any `SignedRecord` | High × low | Accept: the signature covers the bytes as received, never a re-encoding; unknown fields are kept; `verify_strict` |
| Rust and Dart parse the same signed bytes differently | FFI | High × low | Accept: signed records are parsed and verified only in Rust; Dart gets checked records, never Protobuf ([FFI rules](../architecture/ffi-design.md#design-rules-target)) |
| A node serves an older, validly signed profile of a contact (rollback or freeze) | `profile.get` | High × medium | Fix: the client keeps the highest version it has seen per contact and refuses lower ones; it asks two holders with different replica keys and takes the higher version |
| A rolled-back device list or a stale KeyPackage | KeyPackage fetch | Critical × medium | The node checks its current profile and expiry; `rust/client` keeps the highest version seen (`profiles::Profiles`) and verifies the full MLS signature, certified leaf key and lifetime (`Device::create_group`). See [MITM and key verification](#mitm-and-key-verification) |
| An old keep list or a tombstone replayed | `media.keep`, `profile.publish` | Medium × low | Accept: the keep list is versioned and cannot restore a removed hash; a tombstone blocks older versions |
| A node strips or forges the verified mark of a contact | Contact list | High × low | Accept: verification is stored only on the client and bound to the identity key; nodes never see it |
| A forged or poisoned shared deny list | Deny-list exchange | Medium × low | Accept: the exchange is not part of the first release (both switches off); lists are signed with the node key under their own label; received lists are only stored, an operator applies them by hand |

### Replay

Class 19 in detail. In place: `fetch` and `ack` carry a single-use nonce bound to the connection;
every other request is idempotent ([bootstrap](../architecture/bootstrap.md#served-protocol)).
Profiles and keep lists are versioned, and signed times must be within 10 minutes of the node's
clock to refresh liveness, so a replay extends nothing. Signature domains keep one kind of
record from being replayed as another ([Cryptography](#cryptography)).

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Replay a captured `fetch` or `ack` | `mailbox` | High × low | Accept: the nonce is single-use and bound to the connection |
| Replay a `replica_ack` | Node to node | Low × low | Accept: it deletes only ids the device already acked |
| Replay a `revoke` or `replica_revoke` | `mailbox`, node to node | Low × low | Accept: it only revokes the same keys again, until the same expiry |
| A node links a mailbox's blocked contacts by their revoked token keys | `mailbox.revoke` | Low × medium | Accept: it sees token pseudonyms for up to 31 days + one TTL, not identities; at most 4096 keys per mailbox, so revokes cannot grow its storage without bound |
| Close someone else's device mailbox with a `revoke` carrying a self-made device certificate | `mailbox.revoke` | Medium × low | Fix: a close needs the `owner` binding the device signed at linking with its own key, naming the identity that signs the revoke; a certificate alone is not accepted |
| Replay an acked envelope, or repair it back from a replica that missed the ack | `mailbox.put`, repair | Low × medium | Accept: the device drops duplicates by message id and MLS refuses a reused key generation; the copy costs the sender's quota until the TTL |
| Replay an old profile, heartbeat or keep list | `profile.publish`, `search.heartbeat`, `media.keep` | Medium × low | Accept: the version and the 10-minute window; a replay never refreshes liveness |
| Bring back a released attachment by replaying its signed `attach` | `media.attach`, media replica put, repair | Medium × medium | Fix: a node keeps the release hash of a released attachment until the attachment's expiry and refuses an `attach`, replica put or repair with it. Accept: a holder that missed the `release` keeps the attachment until its expiry |
| Replay a `release` | `media.release` | Low × low | Accept: idempotent; only the sender and the recipient know the secret |
| Send a record signed for one node to another | `keep`, `attach`, `publish` | Low × high | Accept: these records are meant for every holder; `fetch` and `ack` are bound to the connection |
| Replay a record from a test network on the main one | Any signed record | Low × low | Accept: it only publishes the owner's own data |
| Reuse TURN credentials | `node.turn` | Low × medium | Accept: the username carries an expiry |
| Replay an MLS message or an old commit | Client | High × low | Accept: OpenMLS deletes used keys and refuses old epochs |
| Feed an old deny list | Manual import | Low × low | Accept: received lists are applied by hand |

### Injection

Class 18 in detail. In place: every SQL value is a bound parameter and `format!` builds SQL only
from in-code constants ([code quality](../development/code-quality.md#sql)); media files are named
by the hex hash of their content; no node code runs a command; peer-supplied text is not logged.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| SQL injection through profile fields, search filters or keys | `search`, `profile`, `media` | High × low | Accept: bound parameters only; no `LIKE` or full-text patterns are built from input |
| Path traversal through a blob name | `media.put`, `shard_put` | High × low | Accept: file paths are the hex hash split into two directory levels; temporary files use a UUID |
| Path tricks in config | `dyappd` config | Low × low | Accept: paths are set by the operator, who already controls the host |
| Log injection: newlines or ANSI escapes in logged values | `tracing` | Low × low | Accept: logs carry peer ids, addresses and error kinds, not peer text; `tracing-subscriber` escapes ANSI sequences in fields |
| Command injection | Packaging scripts, systemd unit, Makefile | High × low | Accept: no node code runs a command and the scripts take no network input |
| Markup, links or URL schemes in profile text | Apps | High × low | Accept: apps show peer data as plain text, with no markdown, HTML, autolinks or WebView, and register no URL schemes ([code quality](../development/code-quality.md)) |
| A malicious preview or profile thumbnail exploits or crashes the image decoder | Chat previews, `Photo.thumbnail` | High × medium | Fix (core done, no client calls it yet): `check_preview` refuses over 24 KiB, a non-JPEG or a frame header over 320 × 320 before decoding, caps decoded JPEG bytes at 1 MiB, accepts RGB and grayscale pixels, and turns a decoder panic into an error (`rust/ffi`, `preview.rs`); the crash found in `zune-jpeg` AVX2 is retained in the fuzz corpus |
| Bidi overrides, zero-width or oversized strings in profile fields | `profile.publish`, `search.publish` | Medium × medium | In place: every profile string has a character limit and the text rule in [Spoofing](#spoofing); `interests` has a count limit |

### Downgrade

Class 20 in detail. In place: nothing is negotiated. Both transports are strong (QUIC with
TLS 1.3, TCP with Noise only), protocol IDs carry no version and protobuf evolves additively, MLS
accepts only ciphersuite 0x0001 ([Cryptography](#cryptography)), and the Argon2id parameters of
the proof of work are fixed in code.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Block UDP to force TCP instead of QUIC | Network | Low × medium | Accept: Noise over TCP is as strong as TLS 1.3 over QUIC |
| Strip the security upgrade or offer a weaker one | multistream-select on TCP | High × low | Accept: Noise is the only upgrade offered and accepted; a plaintext or second security upgrade is never added |
| Force an old protocol version | Protocol IDs | High × low | Accept: there are no versions; a node enforces every rule whatever the client sends ([ADR 0014](../decisions/0014-libp2p-only-node-protocol.md)) |
| Weak MLS ciphersuite | KeyPackages, Welcomes | Critical × low | Accept: only 0x0001 ([Cryptography](#cryptography)) |
| A hostile node lies in `NodeInfo` to downgrade clients: a tiny size limit, a huge proof-of-work minimum, a weaker requirement | `node.info`, replies | Medium × medium | Implemented: `NodeInfo` values are hints only; a client keeps compiled floors, only tightens its requests and skips a node whose values fall outside the accepted range (`dyapp_p2p_net::hints`, [ADR 0016](../decisions/0016-protocol-evolution.md)) |
| A node announces weak Argon2id parameters | Proof of work | High × low | Accept: the parameters are constants, not announced |
| An operator lowers `search.min_profile_pow_bits` or other limits | `dyappd` config | Low × medium | Accept: it weakens only that node; every other node checks gossip against its own minimum, and config validation bounds each value |

### Reflection through an honest node

Class 17 in detail: an attacker makes honest nodes send traffic to a victim it names. Every
node-to-node surface either checks the address it is told to dial or answers only over the
requester's own connection.

| Surface | Defence | Status |
|---------|---------|--------|
| identify `listen_addrs` handed out by both DHTs | An address on the connection's own IP is routed at once; any other one only after a peer-ID-pinned dial reaches that peer, at most 4 pending per peer and 64 per node, each tried once an hour ([protocol](../architecture/bootstrap.md#protocol)) | ✅ Implemented |
| Kademlia `ADD_PROVIDER` addresses | A remote provider is stored only for the search and TURN keys, from a peer ID with the node-ID proof of work, and only when each of its addresses has the IP of a live connection from it; search Kademlia stores no remote providers ([protocol](../architecture/bootstrap.md#protocol)) | ✅ Implemented |
| Kademlia `closer_peers` naming fake peers at a victim's address | Dial guard: no dial to a peer ID without the node-ID proof of work, at most 4 dials in flight and 10 failed dials a minute per /24 or /48, no internal ranges ([dial filtering](../architecture/p2p-networking.md#dial-filtering)) | ✅ Implemented |
| TURN relay to the operator's network or cloud metadata | The coturn example denies internal ranges, multicast and TCP relay ([TURN relay](../manuals/bootstrap.md#turn-relay)) | ✅ Implemented for internal targets |
| Replication fan-out of clients' puts ground onto one node, so it bans an honest sender | `replica_put` only from node-ID proof-of-work peers; at most 50 units/s per destination; a rate refusal of a node's `replica_put` is no strike ([replication](../architecture/bootstrap.md#replication-and-repair), [limits](../architecture/bootstrap.md#limits-and-abuse)) | ✅ Implemented |
| autonat dial-back | Accept: the server dials the observed IP, global addresses only, throttled; the attacker picks only the port | Accepted |
| Search replies, mailbox push, media and mailbox repair, gossipsub | Accept: replies go over the requester's authenticated connection, a search reply is at most 1 MiB, repair answers only the closest or trusted peers, gossipsub runs in strict mode without peer exchange | Accepted |

Accepted residuals:

- Peers behind one CGNAT or NAT IP can claim each other's ports.
- The TURN relay still reaches public addresses, bounded by `user-quota`, `total-quota` and
  `max-bps`.
- A proof-of-work flooder is no longer banned for `replica_put` overload and can crowd honest
  replicas out of a sender's per-destination cap until repair; the peer and IP-group buckets
  still refuse it.
- A claimed public address is still dialled, at up to 10 failed dials a minute per /24 or /48
  per node: each reflected dial costs the attacker a node ID's proof of work.

### Clock manipulation

Class 22 in detail. In place: signed heartbeat and direct keep times must be within 10 minutes of the
node's clock, an attachment's `created` at most 10 minutes ahead, and a replayed record never
extends retention ([Replay](#replay)). Retention and expiry run on the node's clock.
Nodes now report `server_time` in `info` and signed-time `INVALID` replies. Client offset-based
signing remains planned ([clock hints](../architecture/protobuf-schema.md#clock-hints)).

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Future-dated heartbeat, keep or attach to live longer | `profile.heartbeat`, `search.heartbeat`, `media.keep`, `media.attach` | Medium × low | Accept: the 10-minute window bounds the gain to 10 minutes |
| Backdated record | Same | Low × low | Accept: a heartbeat or keep outside the window is `INVALID`; a backdated attach only expires sooner |
| A user's device clock is more than 10 minutes off | Client | Medium × medium | Partial: `info` and a signed-time `INVALID` reply carry the node's clock. Planned: the client signs with the median offset of at least 3 nodes, so one lying node cannot shift it. Until client correction exists, skewed heartbeats and keeps fail and the profile and media can expire |
| Spoofed NTP pushes a node's clock forward or back | Node host | Medium × low | Accept: the node is a cache and other replicas keep the data; [deployment](../manuals/bootstrap.md#clock) asks for authenticated time sync (chrony with NTS) |
| Clock step against per-second rate limits | `limits.*_per_second` | Low × low | Accept: a step resets or extends one second's count |
| Fake send time in a message | Client | Low × medium | Accept: the app shows it as the sender's claim |
| TURN credential expiry on a skewed relay | `node.turn` | Low × low | Accept: the node and its relay belong to one operator |

### Cloud host access

Class 23 in detail. Most nodes run on a rented VM, and whoever controls the hypervisor, the
volumes or the cloud account can reach it without the operator: provider staff, an attacker with
the operator's cloud credentials, or a compelled provider. The node ID is not its operator's
identity, so a stolen node key does not expose the operator's own account; the damage is to the
network and to the users the node serves. In place: `node.key` has mode 0600 and a
[compromise runbook](../manuals/bootstrap.md#node-key-compromise); nodes write no request
logs and no client IP to disk (the only addresses on disk are the peer cache, anchors and IP
groups the operator denied); the TURN secret lives in a separate `EnvironmentFile=`; every record
a node holds is signed by its owner. Nothing is encrypted at rest, the binary is not checked at
start, and a hypervisor can read any VM's memory unless it is a confidential VM. The unit keeps
`dyappd` out of core dumps and swap, and the package seals the node key with `systemd-creds` to
the host key and, where the VM has one, its vTPM; `dyappd` reads the key from a path the host
protects as it chooses ([node key](../manuals/bootstrap.md#node-key)). The other disk-write
risks are accepted for v1 below.

| Attack | Entry point | Impact × likelihood | Verdict |
|--------|-------------|---------------------|---------|
| Copy `node.key` from the volume, a snapshot or a backup and run a node under its peer ID: its proof-of-work DHT position, its local reputation with peers, a seed others pin as `host:port/<peer ID>` | Disk read | High × medium | Fix: `dyappd` reads the key from a path it does not interpret; the package seals it with `systemd-creds --with-key=auto`, so with a vTPM a copied volume alone does not open it ([node key](../manuals/bootstrap.md#node-key)). Accept: without a TPM the host key lies on the same disk, and a host without systemd uses a 0600 file, a passphrase or an encrypted volume; the [runbook](../manuals/bootstrap.md#node-key-compromise) moves a node whose key leaked to a new key, and impersonating a pinned seed also needs its address |
| Read the stores: mailbox addresses, sender keys, write token keys (one contact's envelopes per day, until the TTL), revoked token keys, sizes and times of envelopes, profiles, the search index, media manifests, the deny list | Disk read | Medium × medium | Accept: the same view as the node operator ([privacy](privacy.md)); profiles are public and envelopes from `rust/client` hold only MLS ciphertext |
| Read the TURN secret and mint relay credentials | Disk read (`EnvironmentFile=`), memory | Medium × medium | Accept: coturn's `total-quota` and `max-bps` bound the relay; the operator rotates the secret in the node and coturn together ([TURN relay](../manuals/bootstrap.md#turn-relay)) |
| Dump memory: the node key, the TURN secret, live TLS and Noise session keys, peer IPs of open connections, nonces and rate-limit state | Hypervisor | High × low | Accept: with the session keys and captured traffic the attacker sees the live requests a node operator sees, including client IPs; only a confidential VM (AMD SEV-SNP, Intel TDX) closes this. In place: `LimitCORE=0`, `MemorySwapMax=0` and a non-dumpable process keep the key out of crash dumps and swap, not out of a live hypervisor read |
| Replace the `dyappd` binary, the systemd unit or the config: a backdoored node that logs client IPs against keys, withholds or delays data, serves old profiles, points seeds at the attacker's nodes | Disk write | High × low | Accept: such a node is a hostile node (class 8) under an honest operator's name; it cannot forge signed records or read end-to-end content. No check at start for v1: whoever writes the disk can also replace any check that runs from it; signed releases verified at install are planned ([updates](../manuals/bootstrap.md#updates)), and Secure Boot with dm-verity is the host-side defence for an operator who needs it |
| Plant illegal media or records to expose the operator | Disk write | High × low | Accept for v1: the same legal exposure as class 14; only full-volume encryption (LUKS, a provider-encrypted volume with the operator's key) would make an offline write impossible, and that is the host's choice |
| Restore an old snapshot: an old deny list, a store without recent data, a key the operator already replaced | Snapshot restore | Medium × low | Accept: replication repairs missing data and other nodes refuse older profile versions; a revived replaced key is the key-theft row above, and the runbook's rotation covers it. For v1 the node does not detect that its state went back in time |
| Poison the peer cache or anchors so the node restarts into the attacker's nodes | Disk write | Medium × low | Accept: the configured seeds, pinned to peer IDs, and the routing defences of [ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md) still apply |
| Forge a profile, a message, a media blob or a deny list of another node; read end-to-end content | Any | — | Not possible: these are signed by keys that never reach the node, or encrypted end to end once MLS lands |

## Out of scope

- Global passive adversaries correlating traffic across the network.
- Denial of service at the network layer (volumetric floods below libp2p); operators use their
  hosting's protection.
- A compromised operating system on a user's device.
- Legal procedures, abuse desks and appeals: abuse response is technical only, an accepted
  residual risk ([illegal content](#illegal-content-and-legal-pressure)).
