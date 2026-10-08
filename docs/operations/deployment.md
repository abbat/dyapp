# Deployment Guide

## Overview

> ⚠️ **Not production-ready.** Encryption, signatures and bootstrap
> authentication are stubs; a bootstrap operator and the network see
> plaintext. See [Encryption & Security Status](../security/encryption.md).

dyapp is **fully distributed** — each participant runs their own client. No central servers required (bootstrap servers are optional for peer discovery & message relay).

## Deployment Models

### Model 1: Completely Decentralized (No Bootstrap)

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
- Maximum privacy
- No trust assumptions

**Cons:**
- Slower peer discovery (DHT bootstrap takes time)
- Offline messages require custom relay (implement P2P gossip)

### Model 2: Bootstrap Nodes (Recommended)

```
Dedicated Bootstrap Nodes (1-3 instances)
  ├─ libp2p node protocol (profiles and mailbox today)
  ├─ Profile index (search by age/location)
  ├─ Peer discovery (announce presence)
  └─ Replication (planned; whole records to 5 points, K=6/M=4 for large media, see ADR 0009)

                    ↓ (plaintext today; encryption planned)

Users (iOS/Android/macOS/Linux)
  └─ Mailbox per recipient device on its nodes (single node today; push planned)
```

**Pros:**
- Fast peer discovery (IP:port from bootstrap)
- Offline message delivery
- Profile searching
- Node failure tolerance via erasure coding (planned; `parity` nodes, e.g. 1 of 3 — see [replication math](../architecture/bootstrap.md#replication-strategy-reed-solomon))

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

## Bootstrapping a Node

> **Status:** `dyapp-node` serves `/dyapp/node`, `/dyapp/profile` and `/dyapp/mailbox` over
> libp2p ([served protocol](../architecture/bootstrap.md#served-protocol)); push and replication
> are planned.
> There is no Debian package, systemd unit or production image.

### `dyapp-node`

```bash
cargo run --package dyapp-bootstrap --bin dyapp-node -- --config /etc/dyapp-node.toml
```

The config is optional. Settings come from the defaults, then the TOML file, then
environment variables `DYAPP_NODE__<SECTION>__<KEY>`, whose value is parsed as
TOML (a bare string is taken as is), for example
`DYAPP_NODE__LIMITS__MESSAGE_TTL_HOURS=48` or
`DYAPP_NODE__LISTEN='["/ip4/0.0.0.0/udp/7070/quic-v1"]'`.

```toml
listen = ["/ip4/0.0.0.0/tcp/7070", "/ip4/0.0.0.0/udp/7070/quic-v1"]  # default
external = []          # addresses announced to peers; AutoNAT confirms others
roles = ["store"]      # media, search and turn are not implemented and fail at startup

[storage]
dir = "/var/lib/dyapp-node"   # node key and every store without its own path
# profiles = "/fast/profiles.db"
# messages = "/fast/messages.db"

[limits]
message_ttl_hours = 24
requests_per_second = 100
profiles_max_mb = 1024        # a full store answers FULL to writes
messages_max_mb = 4096
min_free_mb = 512             # free space kept on each store's file system
monthly_traffic_gb = 0        # 0 = no cap; profiles shed from 90 %, mailbox at 100 %
max_connections = 1000
max_connections_per_peer = 4
max_streams = 16              # per connection and protocol

[maintenance]
interval_minutes = 60  # incremental vacuum, WAL checkpoint, PRAGMA optimize
vacuum_pages = 2048    # free 4 KiB pages released per store and run
```

Unknown keys are logged and ignored, so a config written for a newer node does not
stop an older one. At startup the node refuses to run as root, checks addresses,
roles and limits, and checks that every directory is writable. On first start it
creates `node.key` (libp2p key, mode 0600) and `node.id` (its peer ID) in
`storage.dir`. It refuses to start when the key is missing next to existing data
or does not match `node.id`: a new key is a new node, so delete the data to start
from scratch. Expired envelopes are deleted every hour. Logging uses `RUST_LOG`
(e.g. `RUST_LOG=info`).

### Development quick start

`test-peer` is the libp2p client CLI for scripts and manual checks
([commands](../architecture/bootstrap.md#test-client)).

**In Docker (no host Rust toolchain needed).** The network-test image builds `dyapp-node` and
`test-peer` (`docker/Dockerfile.network-test`):

```bash
make prepare            # builds the dev, UI-test and network-test images
make test-integration   # integration tests + two dyapp-node containers on an internal network

# Manual check in a throwaway, network-less container:
docker run --rm --network none --user 999:999 --tmpfs /tmp:rw,exec,mode=1777 \
  -e DYAPP_NODE__STORAGE__DIR=/tmp/node dyapp:network-test bash -c '
    target/debug/dyapp-node &
    sleep 3
    target/debug/test-peer info /ip4/127.0.0.1/tcp/7070
    kill %1'
# → {"max_profile_bytes":1048576,"roles":["ROLE_STORE"],"status":"STATUS_OK"}
```

**With a local Rust 1.99 toolchain:**

```bash
DYAPP_NODE__STORAGE__DIR=/tmp/ai/node cargo run --package dyapp-bootstrap --bin dyapp-node &
cargo run --package dyapp-bootstrap --bin test-peer -- info /ip4/127.0.0.1/tcp/7070
```

Prerequisites: Rust 1.99 (`rust-toolchain.toml`) and a C compiler for the
bundled SQLite, or Docker for the containerised path.

### Multi-node cluster (planned)

Running several `dyapp-node` processes gives independent nodes: there is no
replication or data exchange between them. HA and Reed–Solomon replication
are design targets; see [Bootstrap Servers](../architecture/bootstrap.md).

### Docker and Kubernetes (planned)

There is no production Dockerfile, published image or Kubernetes manifest.
The repository's Dockerfiles (`docker/Dockerfile.dev`, `docker/Dockerfile.network-test`, …)
are test images: they build in debug mode. A production image and manifests are planned.

## Client Configuration (planned)

No platform app connects to a bootstrap server yet, and there is no client API
for configuring bootstrap addresses. The intended behaviour is a
built-in list of bootstrap URLs plus a user-editable setting.

## Monitoring

### Health check (exists)

There is no HTTP endpoint. A node is alive when it answers `/dyapp/node` `info`:

```bash
test-peer info /ip4/127.0.0.1/tcp/7070    # exit 0 and "STATUS_OK" → alive
```

The reply holds roles and limits only: no counts, peer numbers or replication fields.

### Not implemented

These do not exist and need a separate implementation task before they can be
documented as runnable:

- Metrics: planned as log lines only, no `/metrics` endpoint
- Administration: planned as a local CLI writing to `admin.db`, no admin routes
- Request logs: `dyapp-node` logs startup, listen addresses, storage errors and cleanup
  (`RUST_LOG`), not individual requests

## Maintenance

### Data cleanup

Mailbox envelopes expire after `limits.message_ttl_hours` (default 24 h); `dyapp-node` deletes
expired ones every hour. Profiles and tombstones have no expiry yet. See
[Privacy](../security/privacy.md#retention).

### Store compaction

No full `VACUUM` is needed. Every `maintenance.interval_minutes` the node returns up to
`maintenance.vacuum_pages` free pages per store to the file system, truncates the WAL and
refreshes planner statistics; the log line "store maintenance done" shows the free pages left.
A steadily growing number means `vacuum_pages` is too small for the delete rate.

### Backups

There is no online backup: copying the SQLite files while the server writes
to them does not give a consistent snapshot. Stop the process first.

Roundtrip with `dyapp-node` (data in `/tmp/ai/bootstrap`):

```bash
# 1. Stop writers
kill <dyapp-node pid>

# 2. Archive with a relative layout (top-level entry: bootstrap/)
tar czf bootstrap-backup-$(date +%Y%m%d).tar.gz -C /tmp/ai bootstrap

# 3. Restore into an empty test directory and inspect it
mkdir /tmp/restore-test
tar xzf bootstrap-backup-YYYYMMDD.tar.gz -C /tmp/restore-test   # → /tmp/restore-test/bootstrap

# 4. Swap it in, keeping the old copy for rollback, then restart and check
mv /tmp/ai/bootstrap /tmp/ai/bootstrap.old
mv /tmp/restore-test/bootstrap /tmp/ai/bootstrap
DYAPP_NODE__STORAGE__DIR=/tmp/ai/bootstrap cargo run --package dyapp-bootstrap --bin dyapp-node &
test-peer get /ip4/127.0.0.1/tcp/7070 <known peer id>   # must return "STATUS_OK" and the record

# Rollback: stop the server, move bootstrap.old back
```

Scheduled backups, retention and off-host storage are not provided.

### Updates

**Rolling update (planned).** Without cross-node replication, taking a node
offline makes its stored messages and profiles unavailable until it returns;
other nodes do not hold copies. Rolling upgrades need the replication protocol
and a node-failure test first. Target: an upgrade never stops the network
serving; nodes upgrade one at a time, and a changed store format is built next
to the old one while the old one keeps answering
([bootstrap design](../architecture/bootstrap.md#principles)).

## Security

### Network

Every connection is libp2p: TCP with Noise and yamux, or QUIC (TLS 1.3), authenticated by the
node's libp2p key; there is no certificate authority.

### Access Control

**The node protocol is public** (no client authentication):
- Anyone can read any profile; only the owner's signature can store, replace or delete one
- Profile requests are rate-limited per libp2p peer ID (`limits.requests_per_second`, default
  100/s); new keys bypass it, and per-IP limits are planned. Details:
  [served protocol](../architecture/bootstrap.md#served-protocol)

**Admin API (future):**
- Cleanup, monitoring, replication status
- Secured with API key (env var)

## Cost and capacity

Not measured. There is no load test of the bootstrap server. Disk, monthly traffic and
connections are capped by `limits`
([resource guards](../architecture/bootstrap.md#resource-guards)). Multi-node "HA" setups are not possible
yet (see [replication](../architecture/bootstrap.md#replication-strategy-reed-solomon)).

## Troubleshooting

Only what applies to the code that exists today:

| Symptom | Check |
|---------|-------|
| `test-peer info` fails with a timeout or `request failed` | The node is down, the address is wrong (use `/ip4/`, not a host name) or a firewall blocks TCP/UDP 7070 |
| `dyapp-node` exits at startup about a directory | The storage directory is not writable by the node's user; fix permissions or set `DYAPP_NODE__STORAGE__DIR` |
| Port 7070 already in use | Another `dyapp-node` is running; set `DYAPP_NODE__LISTEN` |
| Disk keeps growing | Profiles have no expiry yet (see [Data cleanup](#data-cleanup)) |

There is no systemd unit, start/stop script or service name yet; they ship with the Debian
package (planned). `dyapp-node` logs to stderr.

## Next Steps

Work that turns this guide into a production runbook is tracked in GitHub
Issues: a server binary with configuration, TLS, authentication,
logging/metrics, a TTL cleanup scheduler, and cross-node replication.

## References

- [Bootstrap Server Architecture](../architecture/bootstrap.md)
- [Testing & CI/CD](../testing/README.md)
