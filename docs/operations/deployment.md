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

> **Status:** `dyapp-node` serves `/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox` and
> `/dyapp/media` over libp2p ([served protocol](../architecture/bootstrap.md#served-protocol)).
> A Debian 12 package with a hardened systemd unit exists ([below](#debian-12-package)); there
> is no production container image.

### Debian 12 package

```bash
make deb                                   # → target/deb/dyapp-node_<version>_<arch>.deb
sudo apt install ./dyapp-node_*.deb        # creates the dyapp-node user, enables and starts the unit
sudoedit /etc/dyapp-node.toml              # conffile: kept on upgrade
sudo systemctl restart dyapp-node          # config changes need a restart
sudo systemctl reload dyapp-node           # SIGHUP: reloads <storage.dir>/deny only
journalctl -u dyapp-node                   # logs (RUST_LOG=info)
```

`make deb` builds a release binary in a Debian 12 image (so it runs on glibc 2.36) and
test-installs the package on a clean `debian:bookworm-slim`. The package holds
`/usr/bin/dyapp-node`, `/etc/dyapp-node.toml` and `/lib/systemd/system/dyapp-node.service`.
The unit runs as the system user `dyapp-node` (no shell, no login) with no capabilities,
`NoNewPrivileges`, a read-only system (`ProtectSystem=strict`), no access to `/home`, a private
`/tmp` and write access to `/var/lib/dyapp-node` only. Removing the package stops and disables
the unit; purging keeps the user and `/var/lib/dyapp-node`, because `node.key` is the node's
identity — delete them by hand to retire the node.

The default port 7070 needs no privilege. Stores on other paths or a port below 1024 need a
drop-in (`systemctl edit dyapp-node`):

```ini
[Service]
ReadWritePaths=/big/media
# Only for a port below 1024:
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
AmbientCapabilities=CAP_NET_BIND_SERVICE
```

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
seeds = []             # nodes to join through, e.g. "/dnsaddr/seeds.example.org" or
                       # "/ip4/198.51.100.7/tcp/7070/p2p/12D3Koo..."; peers seen are cached in
                       # <storage.dir>/peers, so later starts do not need them
roles = ["store"]      # add "media" to serve /dyapp/media (needs store); search and turn
                       # are not implemented and fail at startup

[storage]
dir = "/var/lib/dyapp-node"   # node key and every store without its own path
# profiles = "/fast/profiles.db"
# messages = "/fast/messages.db"
# media = "/big/media"        # media role: media.db and the blob files

[limits]
message_ttl_hours = 24
requests_per_second = 100
profiles_max_mb = 1024        # a full store answers FULL to writes
messages_max_mb = 4096
media_max_mb = 10240          # media role; media is never evicted
media_per_owner_mb = 10
media_requests_per_second = 10
min_free_mb = 512             # free space kept on each store's file system
monthly_traffic_gb = 0        # 0 = no cap; media shed from 75 %, profiles 90 %, mailbox 100 %
max_connections = 1000
max_connections_per_peer = 4
max_streams = 16              # per connection and protocol
ip_group_requests_per_second = 1000   # shared by all peers of one IP group
ipv4_prefix = 24              # IP group: leading bits of the remote address
ipv6_prefix = 48
sender_puts_per_second = 10   # envelopes per sender key
strikes_to_ban = 100          # refused floods and bad signatures that ban a peer
ban_minutes = 10

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
make test-integration   # integration tests + three dyapp-node containers on an internal network

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

### Multi-node cluster

Nodes join through `seeds` and find each other over Kademlia. Clients write profiles and
envelopes whole to 5 replica points; nodes repair mailbox replicas between themselves
([replication and repair](../architecture/bootstrap.md#replication-and-repair)). Profile and
media repair and Reed–Solomon for large media are planned.

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
- Profile and mailbox requests are rate-limited per libp2p peer ID (`limits.requests_per_second`,
  default 100/s) and per IP group (`ip_group_requests_per_second`, default 1000/s over a /24 or
  /48); envelope puts also per sender key. Peers that keep hitting the limits or sending bad
  signatures are banned locally for `ban_minutes`. Details:
  [rate limiting](../architecture/bootstrap.md#rate-limiting)
- To stop serving an abuser, add their peer ID, IP group or key hash to `<storage.dir>/deny`
  and send SIGHUP (`systemctl reload dyapp-node`); they get `STATUS_REFUSED` here and
  use other nodes. Details: [deny list](../architecture/bootstrap.md#deny-list)

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

Installed from the Debian package, the service is `dyapp-node.service`; it logs to the journal
(`journalctl -u dyapp-node`). Run by hand, `dyapp-node` logs to stderr.

## Next Steps

Work that turns this guide into a production runbook is tracked in GitHub
Issues: a server binary with configuration, TLS, authentication,
logging/metrics, a TTL cleanup scheduler, and cross-node replication.

## References

- [Bootstrap Server Architecture](../architecture/bootstrap.md)
- [Testing & CI/CD](../testing/README.md)
