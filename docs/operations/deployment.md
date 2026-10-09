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
  └─ Replication (planned; whole records to 5 points, Reed-Solomon K ≤ 6/M = 4 for media above 1 MiB, see ADR 0009)

                    ↓ (plaintext today; encryption planned)

Users (iOS/Android/macOS/Linux)
  └─ Mailbox per recipient device on its nodes (single node today; push planned)
```

**Pros:**
- Fast peer discovery (IP:port from bootstrap)
- Offline message delivery
- Profile searching
- Node failure tolerance: clients write messages and profiles to 5 nodes and nodes repair
  mailboxes; media are not replicated yet ([replication](../architecture/bootstrap.md#replication-and-repair))

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
> A Debian 12 package with a hardened systemd unit ([below](#debian-12-package)) and a minimal
> container image ([below](#container-image)) exist; neither is published yet.

### Debian 12 package

```bash
make deb                                   # → target/deb/dyappd_<version>_<arch>.deb
sudo apt install ./dyappd_*.deb        # creates the dyappd user, enables the unit
sudo -u dyappd dyappd keygen --config /etc/dyappd.toml   # once: about a minute on 2 vCPU
sudoedit /etc/dyappd.toml              # conffile: kept on upgrade; set listen to serve
sudo systemctl start dyappd
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
`/tmp` and write access to `/var/lib/dyappd` only. It is bounded to `MemoryHigh=768M`,
`MemoryMax=1G`, `CPUQuota=150%` and `TasksMax=256`; `limits.max_memory_mb` defaults to
MemoryHigh, so the node sheds requests before the kernel throttles it. On a larger host raise
both in a drop-in. Removing the package stops and disables
the unit; purging keeps the user and `/var/lib/dyappd`, because `node.key` is the node's
identity — delete them by hand to retire the node.

The default port 7070 needs no privilege. Another `storage.dir` or a port below 1024 needs a
drop-in (`systemctl edit dyappd`):

```ini
[Service]
ReadWritePaths=/srv/dyappd
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
`DYAPPD__LISTEN='["[2001:db8::7]:7070"]'`, then command-line options: each key is an option
named `--<section>.<key>` with `-` for `_`, such as `--limits.max-connections 500`,
`--storage.dir=/srv/dyappd` or `--listen '[::]:7070' --listen 0.0.0.0:7070` (a list option is
given once per item and replaces the list). An unknown option stops the node. `dyappd --help`
lists every option with its default; the list is generated from the packaged
`/etc/dyappd.toml` (`rust/bootstrap/dyappd.toml`), and a test keeps that file equal to the
defaults in the code. Keep `turn.secret` off the command line: other users see it in `ps`.

`listen` and `external` take `host:port` addresses with an IP, not a host name; an IPv6
address goes in brackets. Each address serves both transports on its port: TCP and QUIC over
UDP, so open both in the firewall. Peers use QUIC where UDP gets through and TCP otherwise. The
default listens on loopback only (`[::1]:7070` and `127.0.0.1:7070`); a node that serves the
network sets `listen = ["[::]:7070", "0.0.0.0:7070"]`. The node binds IPv6 sockets IPv6-only,
so `[::]` alone does not accept IPv4: a dual-stack node lists both. An address that does not open (`[::]` on a host with IPv6
disabled, a port already taken) is logged as `not listening` and skipped; the node exits only
when none opens.

With `external` empty, nothing is announced up front: peers learn the listen addresses through
identify, and the address they see the node from becomes external once AutoNAT confirms it is
reachable. Until then the node is a DHT client: it queries the DHT but serves no part of it. Set
`external` on a host whose public address is known, or behind port forwarding.

`seeds` are `host:port` too, and the host may also be a DNS name: `seed.example.org:7070`,
`[2001:db8::7]:7070`. The node dials each seed over TCP and QUIC; a name with several A/AAAA
records is tried address by address. No peer ID is needed: the node learns it on connect, so
it does not check that a seed is the node the operator meant. That only decides the first
peers: the routing table then fills from the network, with the
[Sybil and eclipse defences](../decisions/0008-sybil-and-eclipse-defences.md). Libp2p
multiaddrs (`/dnsaddr/...`, `/ip6/.../p2p/...`) are refused at startup.

Roles are a list, so `roles = ["store", "media", "turn"]` combines them. `store` serves
profiles, mailboxes and the DHT; `media` serves media blobs and `turn` hands out credentials
for a coturn relay ([TURN relay](#turn-relay)), and neither includes `store`: both are refused
at startup without it in the list. `search` is planned and refused at startup. A node without
a role answers its requests `UNSUPPORTED`.

```toml
listen = ["[::1]:7070", "127.0.0.1:7070"]  # default: loopback; ["[::]:7070", "0.0.0.0:7070"] serves
external = []          # addresses announced to peers, e.g. "[2001:db8::7]:7070";
                       # empty: the address AutoNAT confirms
seeds = []             # nodes to join through, e.g. "seed.example.org:7070" or
                       # "[2001:db8::7]:7070"; peers seen are cached in
                       # <storage.dir>/peers, so later starts do not need them; the last 3
                       # outbound peers that answered, in <storage.dir>/anchors, are dialled first
roles = ["store"]      # e.g. ["store", "media", "turn"]: media serves /dyapp/media, turn
                       # hands out TURN credentials, both need store in the list;
                       # search is not implemented and fails at startup

[storage]
dir = "/var/lib/dyappd"   # node.key, node.id, profiles.db, messages.db, media.db,
                          # deny.db, peers, anchors; media blob files in data/

[limits]
message_ttl_hours = 24
profile_ttl_days = 30         # after the owner's last signed request
requests_per_second = 100
profiles_max_mb = 1024        # a full store answers FULL to writes; disk use stays below
                              # the sum of the store sizes plus SQLite overhead
messages_max_mb = 4096
media_max_mb = 10240          # media role; media is never evicted (eviction planned)
media_per_owner_mb = 10
attachment_retention_hours = 168  # unreleased chat attachments
media_requests_per_second = 10
min_free_mb = 512             # free space kept on the file system of storage.dir
bytes_per_second = 0          # 0 = no limit; media shed from 75 %, profiles 90 %, mailbox 100 %
max_connections = 1000
max_connections_per_peer = 4
max_streams = 16              # per connection and protocol
max_memory_mb = 768           # RSS: requests refused from 80 %, dropped and new
                              # connections refused at 100 %; 0 = no limit
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

### Container image

```bash
make image                             # → dyappd:latest, then a read-only test run
docker volume create dyappd
run="docker run --rm --read-only --cap-drop ALL --security-opt no-new-privileges:true -v dyappd:/var/lib/dyappd"
$run dyappd:latest keygen              # once: about a minute on 2 vCPU
$run -d --name dyappd -p 7070:7070/tcp -p 7070:7070/udp \
    -e DYAPPD__SEEDS='["seed.example.org:7070"]' dyappd:latest
docker exec dyappd dyappd deny add <entry> [note]   # applied within 10 s
docker exec dyappd dyappd status
docker logs dyappd
```

`docker/Dockerfile.dyappd` builds the release binary in the Debian 12 image and copies it with
the glibc libraries it links onto an empty base: no shell, no package manager, no `/etc`. The
node runs as UID 65532 and writes only `/var/lib/dyappd`, its `storage.dir` and the image's one
volume; Docker supplies `/etc/resolv.conf` and `/etc/hosts`. So the root can be read-only and
every capability dropped. A new named volume takes the directory's owner and mode 0700 from
the image; a bind mount needs `chown 65532:65532`. The image
sets `listen` to `["[::]:7070", "0.0.0.0:7070"]`; the other keys are `DYAPPD__` variables,
`--<section>.<key>` options after the image name, or a config file mounted read-only and named
with `--config`. Resource bounds are Docker's: `--memory 1g` with the default
`limits.max_memory_mb = 768` sheds requests before the limit. `make image` runs the image
read-only with no network, makes a key, starts the node, edits the deny list and checks that
the image has no shell.

The image is not published to a registry, and there are no Kubernetes manifests yet. The other
Dockerfiles in `docker/` are test images built in debug mode.

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

`storage.dir` (`/var/lib/dyappd` from the package) holds `node.key`, the node's identity, and
`node.id`; the stores `profiles.db`, `messages.db` and `media.db`, with the media blob files in
`data/`; `deny.db`, the operator's deny list; and `peers` and `anchors`, the peer cache, which
the node rebuilds. A `traffic` file left by an older version is no longer read and can be
deleted. A media store an older version kept in `media/` is moved to `media.db` and `data/` on
the first start. The keys `storage.profiles`, `storage.messages` and `storage.media` are
removed, and a config that sets one is refused at startup: move that store into
`storage.dir` first.

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

Not measured. There is no load test of the bootstrap server. Disk, the byte rate, memory and
connections are capped by `limits`
([resource guards](../architecture/bootstrap.md#resource-guards)). Several nodes survive the loss of one for
messages and profiles, not for media
([replication](../architecture/bootstrap.md#replication-and-repair)).

## Troubleshooting

Only what applies to the code that exists today:

| Symptom | Check |
|---------|-------|
| `test-peer info` fails with a timeout or `request failed` | The node is down, the address is wrong (use `/ip4/`, not a host name) or a firewall blocks TCP/UDP 7070 |
| `dyappd` exits at startup about a directory | The storage directory is not writable by the node's user; fix permissions or set `DYAPPD__STORAGE__DIR` |
| `test-peer info` works on the host but times out from elsewhere | `listen` is the loopback default; set `listen = ["[::]:7070", "0.0.0.0:7070"]` |
| `not listening` warning or `no address could be opened` | Port 7070 is taken (another `dyappd`?) or IPv6 is disabled on the host; set `DYAPPD__LISTEN` |
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

Planned for the node: public seed nodes and erasure-coded media; see
[Bootstrap](../architecture/bootstrap.md).

## References

- [Bootstrap Server Architecture](../architecture/bootstrap.md)
- [Testing & CI/CD](../testing/README.md)
