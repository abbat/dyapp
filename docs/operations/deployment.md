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

> **Status:** `dyappd` serves `/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox` and
> `/dyapp/media` over libp2p ([served protocol](../architecture/bootstrap.md#served-protocol)).
> A Debian 12 package with a hardened systemd unit exists ([below](#debian-12-package)); there
> is no production container image.

### Debian 12 package

```bash
make deb                                   # → target/deb/dyappd_<version>_<arch>.deb
sudo apt install ./dyappd_*.deb        # creates the dyappd user, enables the unit
sudo -u dyappd dyappd keygen --config /etc/dyappd.toml   # once: about a minute on 2 vCPU
sudo systemctl start dyappd
sudoedit /etc/dyappd.toml              # conffile: kept on upgrade
sudo systemctl restart dyappd          # config changes need a restart
sudo -u dyappd dyappd deny add <entry> [note] --config /etc/dyappd.toml  # applied within 10 s
sudo -u dyappd dyappd status --config /etc/dyappd.toml   # what the stores hold, read-only
journalctl -u dyappd                   # logs (RUST_LOG=info)
```

`make deb` builds a release binary in a Debian 12 image (so it runs on glibc 2.36) and
test-installs the package on a clean `debian:bookworm-slim`. The package holds
`/usr/bin/dyappd`, `/etc/dyappd.toml` and `/lib/systemd/system/dyappd.service`.
The unit runs as the system user `dyappd` (no shell, no login) with no capabilities,
`NoNewPrivileges`, a read-only system (`ProtectSystem=strict`), no access to `/home`, a private
`/tmp` and write access to `/var/lib/dyappd` only. Removing the package stops and disables
the unit; purging keeps the user and `/var/lib/dyappd`, because `node.key` is the node's
identity — delete them by hand to retire the node.

The default port 7070 needs no privilege. Stores on other paths or a port below 1024 need a
drop-in (`systemctl edit dyappd`):

```ini
[Service]
ReadWritePaths=/big/media
# Only for a port below 1024:
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
AmbientCapabilities=CAP_NET_BIND_SERVICE
```

### `dyappd`

```bash
cargo run --package dyapp-bootstrap --bin dyappd -- keygen --config /etc/dyappd.toml  # once
cargo run --package dyapp-bootstrap --bin dyappd -- --config /etc/dyappd.toml
```

The config is optional. Settings come from the defaults, then the TOML file, then
environment variables `DYAPPD__<SECTION>__<KEY>`, whose value is parsed as
TOML (a bare string is taken as is), for example
`DYAPPD__LIMITS__MESSAGE_TTL_HOURS=48` or
`DYAPPD__LISTEN='["/ip4/0.0.0.0/udp/7070/quic-v1"]'`.

```toml
listen = ["/ip4/0.0.0.0/tcp/7070", "/ip4/0.0.0.0/udp/7070/quic-v1"]  # default
external = []          # addresses announced to peers; AutoNAT confirms others
seeds = []             # nodes to join through, e.g. "/dnsaddr/seeds.example.org" or
                       # "/ip4/198.51.100.7/tcp/7070/p2p/12D3Koo..."; peers seen are cached in
                       # <storage.dir>/peers, so later starts do not need them; the last 3
                       # outbound peers that answered, in <storage.dir>/anchors, are dialled first
roles = ["store"]      # add "media" to serve /dyapp/media and "turn" to hand out TURN
                       # credentials (both need store); search is not implemented
                       # and fails at startup

[storage]
dir = "/var/lib/dyappd"   # node key and every store without its own path
# profiles = "/fast/profiles.db"
# messages = "/fast/messages.db"
# media = "/big/media"        # media role: media.db and the blob files

[limits]
message_ttl_hours = 24
profile_ttl_days = 30         # after the owner's last signed request
requests_per_second = 100
profiles_max_mb = 1024        # a full store answers FULL to writes
messages_max_mb = 4096
media_max_mb = 10240          # media role; media is never evicted
media_per_owner_mb = 10
attachment_retention_hours = 168  # unreleased chat attachments
media_requests_per_second = 10
min_free_mb = 512             # free space kept on each store's file system
monthly_traffic_gb = 0        # 0 = no cap; media shed from 75 %, profiles 90 %, mailbox 100 %
bytes_per_second = 0          # 0 = no limit; shed in the same order within each second
max_connections = 1000
max_connections_per_peer = 4
max_streams = 16              # per connection and protocol
max_memory_mb = 0             # refuse new connections above this RSS; 0 = no limit
ip_group_requests_per_second = 1000   # shared by all peers of one IP group
ipv4_prefix = 24              # IP group: leading bits of the remote address
ipv6_prefix = 48
sender_puts_per_second = 10   # envelopes per sender key
strikes_to_ban = 100          # refused floods and bad signatures that ban a peer
ban_minutes = 10

[maintenance]
interval_minutes = 60  # incremental vacuum, WAL checkpoint, PRAGMA optimize
vacuum_pages = 2048    # free 4 KiB pages released per store and run

[network]
id_pow_bits = 22       # node-ID proof of work; lower it on test networks only
distinct_outbound_groups = true  # one routed peer per /16 (IPv6 /32) in each k-bucket
storage_trust_minutes = 60       # a new peer gets replicas after this; 0 on test networks
share_deny_list = false          # answer other nodes with the signed deny list (no notes)
accept_deny_lists = false        # fetch and store other nodes' lists; no action taken

[turn]                 # turn role only
urls = []              # e.g. "turn:turn.example.org:3478", "turns:turn.example.org:5349"
secret = ""            # coturn static-auth-secret; better DYAPPD__TURN__SECRET
credential_minutes = 60
```

Unknown keys are logged and ignored, so a config written for a newer node does not
stop an older one. At startup the node refuses to run as root, checks addresses,
roles and limits, and checks that every directory is writable. `dyappd keygen`
creates `node.key` (libp2p key, mode 0600) and `node.id` (its peer ID) in
`storage.dir` and prints the peer ID. The key carries the node-ID proof of work: SHA-256 of
the peer ID starts with `network.id_pow_bits` zero bits, about a minute on 2 vCPU
([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)). Keygen refuses when a key or any
data already exists. Turn `network.distinct_outbound_groups` off only on a network whose
nodes share a /16, such as a LAN or a test network. The node does not start without the key,
with a key that lacks the proof of work or with one that does not match `node.id`: a new key is
a new node, so delete the data to start from scratch. Keep a copy of `node.key` safe. Expired envelopes are deleted every hour. Logging uses `RUST_LOG`
(e.g. `RUST_LOG=info`).

### Development quick start

`test-peer` is the libp2p client CLI for scripts and manual checks
([commands](../architecture/bootstrap.md#test-client)).

**In Docker (no host Rust toolchain needed).** The network-test image builds `dyappd` and
`test-peer` (`docker/Dockerfile.network-test`):

```bash
make prepare            # builds the dev, UI-test and network-test images
make test-integration   # integration tests + three dyappd containers on an internal network

# Manual check in a throwaway, network-less container:
docker run --rm --network none --user 999:999 --tmpfs /tmp:rw,exec,mode=1777 \
  -e DYAPPD__STORAGE__DIR=/tmp/node dyapp:network-test bash -c '
    export DYAPPD__NETWORK__ID_POW_BITS=8
    target/debug/dyappd keygen
    target/debug/dyappd &
    sleep 3
    target/debug/test-peer info /ip4/127.0.0.1/tcp/7070
    kill %1'
# → {"max_profile_bytes":1048576,"roles":["ROLE_STORE"],"status":"STATUS_OK"}
```

**With a local Rust 1.99 toolchain:**

```bash
export DYAPPD__STORAGE__DIR=/tmp/ai/node DYAPPD__NETWORK__ID_POW_BITS=8
cargo run --package dyapp-bootstrap --bin dyappd -- keygen
cargo run --package dyapp-bootstrap --bin dyappd &
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

- Metrics: beyond the hourly `node status`, guard and connection log lines, none; no `/metrics`
  endpoint
- Request logs: `dyappd` logs startup, listen addresses, storage errors and cleanup
  (`RUST_LOG`), not individual requests

## Maintenance

### Data cleanup

Mailbox envelopes expire after `limits.message_ttl_hours` (default 24 h); `dyappd` deletes
expired ones every hour. Profiles and tombstones go once their owner has signed nothing for
`limits.profile_ttl_days` (default 30). See
[Privacy](../security/privacy.md#retention).

### Store compaction

No full `VACUUM` is needed. Every `maintenance.interval_minutes` the node returns up to
`maintenance.vacuum_pages` free pages per store to the file system, truncates the WAL and
refreshes planner statistics; the log line "store maintenance done" shows the free pages left.
A steadily growing number means `vacuum_pages` is too small for the delete rate.

### Backups

There is no online backup: copying the SQLite files while the server writes
to them does not give a consistent snapshot. Stop the process first.

Roundtrip with `dyappd` (data in `/tmp/ai/bootstrap`):

```bash
# 1. Stop writers
kill <dyappd pid>

# 2. Archive with a relative layout (top-level entry: bootstrap/)
tar czf bootstrap-backup-$(date +%Y%m%d).tar.gz -C /tmp/ai bootstrap

# 3. Restore into an empty test directory and inspect it
mkdir /tmp/restore-test
tar xzf bootstrap-backup-YYYYMMDD.tar.gz -C /tmp/restore-test   # → /tmp/restore-test/bootstrap

# 4. Swap it in, keeping the old copy for rollback, then restart and check
mv /tmp/ai/bootstrap /tmp/ai/bootstrap.old
mv /tmp/restore-test/bootstrap /tmp/ai/bootstrap
DYAPPD__STORAGE__DIR=/tmp/ai/bootstrap cargo run --package dyapp-bootstrap --bin dyappd &
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
- To stop serving an abuser or a media blob, add their peer ID, IP group, key hash or the blob's
  SHA-256 with `sudo -u dyappd dyappd deny add <entry> [note] --config /etc/dyappd.toml`
  (`deny remove`, `deny list`); the running node applies it within 10 seconds, and they get
  `STATUS_REFUSED` here and use other nodes. Details: [deny list](../architecture/bootstrap.md#deny-list)
- A shared list tells other operators the peer IDs, IP groups and hashes you refuse, so
  `share_deny_list` is off by default. Lists received with `accept_deny_lists` are only stored:
  review them with `dyappd deny received` and add what you agree with yourself
  ([exchange](../architecture/bootstrap.md#deny-list-exchange))

`sudo -u dyappd dyappd status --config /etc/dyappd.toml` prints the peer ID and what the
stores hold (profiles, envelopes, media blobs and bytes, deny entries, cached peers), read-only.
There is no admin HTTP API.

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
| `dyappd` exits at startup about a directory | The storage directory is not writable by the node's user; fix permissions or set `DYAPPD__STORAGE__DIR` |
| Port 7070 already in use | Another `dyappd` is running; set `DYAPPD__LISTEN` |
| Disk keeps growing | Profiles stay for `limits.profile_ttl_days` after their owner's last request; lower it or the store quotas (see [Data cleanup](#data-cleanup)) |

Installed from the Debian package, the service is `dyappd.service`; it logs to the journal
(`journalctl -u dyappd`). Run by hand, `dyappd` logs to stderr.

## TURN relay

The `turn` role hands out credentials for a coturn relay that runs next to the node; `dyappd`
relays nothing itself. Install coturn (`apt-get install coturn` on Debian 12) and set in
`/etc/turnserver.conf`:

```
use-auth-secret
static-auth-secret=<the same secret as turn.secret>
realm=turn.example.org
total-quota=100
user-quota=4
max-bps=1000000
no-cli
```

Keep the secret out of the config file: pass `DYAPPD__TURN__SECRET` through a systemd
`EnvironmentFile=` with mode 0600. Open the listening port (3478 UDP and TCP, 5349 for TLS) and
the relay port range (`min-port`/`max-port`, 49152–65535 by default). At startup the node
refuses the role without a secret or with a URL that does not start with `turn:` or `turns:`.
Credentials expire after `turn.credential_minutes`; rotating the secret needs both services
restarted together.

## Next Steps

Planned for the node: public seed nodes and erasure-coded storage; see
[Bootstrap](../architecture/bootstrap.md).

## References

- [Bootstrap Server Architecture](../architecture/bootstrap.md)
- [Testing & CI/CD](../testing/README.md)
