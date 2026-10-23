# Deployment Guide

## Overview

> ⚠️ **Not production-ready.** Message encryption is a stub: a bootstrap operator and the
> network see plaintext. The node checks Ed25519 signatures: the owner's on profiles, the
> sender's on envelopes and the device's on mailbox reads. See [Encryption & Security Status](../security/encryption.md).

dyapp is **fully distributed** — each participant runs their own client. No central servers required (bootstrap servers are optional for peer discovery & message relay).

## Deployment Models

### Model 1: Completely Decentralized (No Bootstrap)

Planned: clients do not run a libp2p node yet, so today every client goes through bootstrap
nodes (Model 2).

```
Device A (iOS)
  ├─ P2P client
  ├─ Signed profile sync
  ├─ E2E encryption (planned)
  └─ DHT node

Device B (Android)
  ├─ P2P client
  ├─ Signed profile sync
  ├─ E2E encryption (planned)
  └─ DHT node

Device C (macOS)
  ├─ P2P client
  ├─ Signed profile sync
  ├─ E2E encryption (planned)
  └─ DHT node

Peer discovery: DHT mesh (no central coordination)
Message relay: P2P direct or gossip via other peers
```

**Pros:**
- Zero infrastructure cost
- No operator holds profiles or mailboxes; peers still see each other's IP addresses

**Cons:**
- Slower peer discovery (DHT bootstrap takes time)
- Offline messages require custom relay (implement P2P gossip)

### Model 2: Bootstrap Nodes (Recommended)

```
Dedicated Bootstrap Nodes (1-3 instances)
  ├─ libp2p node protocol (profiles and mailbox today)
  ├─ Profile index (search by age/location)
  ├─ Peer discovery (announce presence)
  └─ Replication (whole records to 5 points; Reed-Solomon K ≤ 6/M = 4 for larger media, see ADR 0009)

                    ↓ (plaintext today; encryption planned)

Users (iOS/Android/macOS/Linux)
  └─ Mailbox per recipient device on its five replica nodes; a watching fetch gets new envelopes pushed
```

**Pros:**
- Fast peer discovery (IP:port from bootstrap)
- Offline message delivery
- Profile searching
- Node failure tolerance: acceptors replicate messages, profiles and media up to 1 MiB to five
  DHT points and acknowledge two distinct stores; larger media up to 6 MiB confirms two
  manifest holders and K shard indices. Nodes repair mailboxes and profiles.
  Media uses the store DHT, so enable both `store` and `media` on its selected holders.
  Owner keeps repair missing media copies/shards within the shared 75 % traffic budget
  ([replication](../architecture/bootstrap.md#replication-and-repair)).

**Cons:**
- Need to operate 1-3 servers (~$10-50/month)
- Bootstrap sees peer IDs, IPs, who messages whom, age, location — and all content until client crypto exists ([Privacy](../security/privacy.md))

### Model 3: Hybrid (DHT + Optional Bootstrap)

```
Primary: DHT peer discovery (free, takes 30-60 sec)
Fallback: Bootstrap nodes (fast, 1-2 sec)

Example: User opens app
  1. Try DHT for 2 seconds
  2. If slow, query bootstrap
  3. Both return results, user sees merged list
```

## Running a node

Installing, configuring and operating a `dyappd` bootstrap node (the Debian 12 package, the
container image, the node key, every configuration option, monitoring, updates, backups and
security) is described in the
[Bootstrap node operator manual](../manuals/bootstrap.md)
([Russian](../manuals/bootstrap.ru.md)). This page keeps the network-level view and the
developer setup.

### Getting a release

A `vX.Y.Z` tag publishes `dyappd`, a tested Linux desktop client DEB and a
smoke-tested Windows portable ZIP from CI
([ADR 0020](../decisions/0020-dyappd-release-channels.md)); no release is published yet,
so until the first tag, build from a tagged commit with `make deb`.

| Artifact | Where |
|----------|-------|
| Debian 12 package `dyappd_X.Y.Z_amd64.deb` | GitHub Releases of `abbat/dyapp` |
| Linux desktop package `dyapp_X.Y.Z_amd64.deb` | Same GitHub Release, after package-install and UI smoke tests |
| Windows portable ZIP `dyapp-X.Y.Z-x86_64-windows.zip` | Same release, after native Windows UI smoke |
| Tarball `dyappd-X.Y.Z-x86_64-linux-gnu.tar.gz` | GitHub Releases |
| SPDX SBOM `dyappd-X.Y.Z.spdx.json` and `SHA256SUMS` | GitHub Releases |
| Container image | `ghcr.io/abbat/dyappd:X.Y.Z` |

Check the files and the image before installing; every one carries a signed build provenance
attestation from this repository's `release.yml`:

```bash
sha256sum -c SHA256SUMS
gh attestation verify dyappd_X.Y.Z_amd64.deb --repo abbat/dyapp
gh attestation verify dyapp_X.Y.Z_amd64.deb --repo abbat/dyapp
gh attestation verify dyapp-X.Y.Z-x86_64-windows.zip --repo abbat/dyapp
gh attestation verify oci://ghcr.io/abbat/dyappd:X.Y.Z --repo abbat/dyapp
```

Operators answering an abuse report or a lawful-access request: [Abuse reports and legal
requests](abuse-and-legal-requests.md).

## Development

### Development quick start

`dyappd info` and the other [client commands](../architecture/bootstrap.md#client-commands) are the
libp2p client for scripts and manual checks.

**In Docker (no host Rust toolchain needed).** The network-test image builds `dyappd`
(`tests/docker/Dockerfile.network-test`):

```bash
make prepare            # builds the dev, UI-test and network-test images
make test-integration   # integration tests, three dyappd containers on an internal network,
                        # and a 5-minute memory plateau on release binaries

# Manual check in a throwaway, network-less container:
docker run --rm --network none --user 1000:1000 --tmpfs /tmp:rw,exec,mode=1777 \
  -e DYAPPD_STORAGE_DIR=/tmp/node dyapp:network-test bash -c '
    export DYAPPD_NETWORK_ID_POW_BITS=8
    build/debug/dyappd keygen
    build/debug/dyappd &
    sleep 3
    build/debug/dyappd info [::1]:7070
    kill %1'
# → {"max_profile_bytes":1048576,"roles":["ROLE_STORE","ROLE_SEARCH"],"status":"STATUS_OK"}
```

**With a local Rust 1.99 toolchain:**

```bash
export DYAPPD_STORAGE_DIR=/tmp/ai/node DYAPPD_NETWORK_ID_POW_BITS=8
cargo run --package dyapp-bootstrap --bin dyappd -- keygen
cargo run --package dyapp-bootstrap --bin dyappd &
cargo run --package dyapp-bootstrap --bin dyappd -- info [::1]:7070
```

Prerequisites: Rust 1.99 (`rust-toolchain.toml`) and a C compiler for the
bundled SQLite, or Docker for the containerised path.

### Multi-node cluster

Nodes join through `seeds` and find each other over Kademlia. A client writes a profile or an
envelope to one node, which replicates it whole to 5 DHT points; nodes repair mailbox replicas between themselves
([replication and repair](../architecture/bootstrap.md#replication-and-repair)). Profile repair
and Reed-Solomon media up to 6 MiB are implemented. Owner-signed keeps trigger batched media
repair of missing items; no repair runs while the owner remains offline.

## Client Configuration (planned)

No platform app connects to a bootstrap server yet, and there is no client API
for configuring bootstrap addresses. The intended behaviour is a
built-in list of bootstrap URLs plus a user-editable setting.

## Next Steps

Planned work for the node is listed in
[Bootstrap: Limitations & Future](../architecture/bootstrap.md#limitations--future).

## References

- [Bootstrap node operator manual](../manuals/bootstrap.md)
- [Bootstrap Server Architecture](../architecture/bootstrap.md)
- [Testing & CI/CD](../testing/README.md)
