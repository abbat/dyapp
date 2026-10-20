# Deployment Guide

## Overview

> ⚠️ **Not production-ready.** Message encryption is a stub: a bootstrap operator and the
> network see plaintext. The node checks Ed25519 signatures: the owner's on profiles, the
> sender's on envelopes and the device's on mailbox reads. See [Encryption & Security Status](../security/encryption.md).

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

## Bootstrapping a Node

> **Status:** `dyappd` serves `/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`,
> `/dyapp/mailbox-push`, `/dyapp/media`, `/dyapp/search` and `/dyapp/inventory` over libp2p
> ([served protocol](../architecture/bootstrap.md#served-protocol)).
> A Debian 12 package with a hardened systemd unit ([below](#debian-12-package)) and a minimal
> container image ([below](#container-image)) exist; neither is published yet.

### Debian 12 package

```bash
make deb                                   # → target/deb/dyappd_<version>_<arch>.deb
sudo apt install ./dyappd_*.deb        # creates the dyappd user, enables the unit
sudo install -d -m 0700 /etc/credstore.encrypted
sudo -u dyappd dyappd keygen - --config /etc/dyappd.toml |   # once: about a minute on 2 vCPU
  sudo systemd-creds encrypt --with-key=auto --name=node.key - /etc/credstore.encrypted/node.key
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
both in a drop-in. `LimitCORE=0` and `MemorySwapMax=0` (cgroup v2), with `dyappd` making itself
non-dumpable at start, keep the node key out of core dumps and swap; run the node without swap.
A hypervisor can still read the memory of a running VM: only a confidential VM (AMD SEV-SNP,
Intel TDX) prevents that. `LoadCredentialEncrypted=node.key` decrypts the
[sealed node key](#node-key) for the service. Removing the package stops and disables
the unit; purging keeps the user, `/var/lib/dyappd` and `/etc/credstore.encrypted/node.key`,
because the key is the node's identity — delete them by hand to retire the node.

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

`seeds` take `host:port` or `host:port/<peer ID>`; the host may be an IPv4/IPv6 address or a
DNS name, for example `seed.example.org:7070` or `[2001:db8::7]:7070`. Append the peer ID
printed by that seed's `dyappd keygen` (also stored in its `node.id`) to pin its transport key.
Both TCP and QUIC dials carry the pin; a different key is refused and logged at warn.
A malformed peer ID is rejected at startup. A name with several A/AAAA records is tried
address by address. Plain `host:port` remains supported and learns the ID on connect, so whoever
controls that DNS name or host controls those first peers. Pinning only decides the first peers:
the routing table then fills from the network, with the
[Sybil and eclipse defences](../decisions/0008-sybil-and-eclipse-defences.md). Libp2p
multiaddrs (`/dnsaddr/...`, `/ip6/.../p2p/...`) are refused at startup.

Roles are a list, so `roles = ["store", "media", "turn"]` combines them. `store` serves
profiles, mailboxes and the DHT; `media` serves media blobs and `turn` hands out credentials
for a coturn relay ([TURN relay](#turn-relay)), and the experimental `search` serves profile
search ([Search](../architecture/bootstrap.md#search)); none includes `store`, and each is refused
at startup without it in the list. A search publish needs profile proof of work
(`search.min_profile_pow_bits`); the defences against query and fake-profile floods are planned
([threat model](../security/threat-model.md)). On the public network a search node keeps all 16
topics: a `search.topics` subset is refused at startup. Its answers are a local, possibly partial
sample ([Search](../architecture/bootstrap.md#search)). A node without a role answers its requests
`UNSUPPORTED`.

```toml
listen = ["[::1]:7070", "127.0.0.1:7070"]  # default: loopback; ["[::]:7070", "0.0.0.0:7070"] serves
external = []          # addresses announced to peers, e.g. "[2001:db8::7]:7070";
                       # empty: the address AutoNAT confirms
seeds = []             # nodes to join through, e.g. "seed.example.org:7070" or
                       # "[2001:db8::7]:7070"; peers seen are cached in
                       # <storage.dir>/peers, so later starts do not need them; the last 3
                       # outbound peers that answered, in <storage.dir>/anchors, are dialled first
roles = ["store"]      # e.g. ["store", "media", "turn"]: media serves /dyapp/media, turn
                       # hands out TURN credentials, search (experimental) answers
                       # profile search; all need store in the list

[storage]
dir = "/var/lib/dyappd"   # node.key, node.id, network, profiles.db, messages.db, media.db,
                          # deny.db, profiles-idx.db or profiles-idx-vN.db and
                          # search.lock (search), peers, anchors;
                          # media blob files in data/
key = ""                  # node key file; empty: the unit's node.key credential if
                          # there is one, else <dir>/node.key

[limits]
message_ttl_hours = 24
profile_ttl_days = 30         # profile newer publish/heartbeat; media signed keep/attach time
requests_per_second = 100
profiles_max_mb = 1024        # a full store answers FULL to writes; disk use stays below
                              # the sum of the store sizes plus SQLite overhead
messages_max_mb = 4096
signal_max_kb = 2048          # signals (likes, views) per mailbox, the oldest evicted; at least 2
mailbox_pool_kb = 1024        # tokenless messages per mailbox with require_write_tokens; at least 101
tokenless_share_pct = 20      # tokenless bytes store-wide, percent of messages_max_mb
require_write_tokens = false  # tokenless messages go to the pool, not the 10 MiB
media_max_mb = 10240          # media role; evict attachments then oldest owners to 95%
media_per_owner_mb = 10       # billed bytes: whole copies × 5, shards ceil(bytes × (K+4)/K)
attachment_retention_hours = 168  # unreleased chat attachments
# media_cache_minutes = 10    # planned: assembled blobs kept after their last read
media_requests_per_second = 10
min_free_mb = 512             # free space kept on the file system of storage.dir
bytes_per_second = 0          # 0 = no limit; media shed from 75 %, profiles 90 %, mailbox 100 %
max_connections = 1000
max_connections_per_peer = 4
max_connections_per_ip_group = 32  # incoming pending + established; outgoing exempt
max_streams = 16              # per connection and protocol
max_memory_mb = 768           # RSS: requests refused from 80 %, dropped and new
                              # connections refused at 100 %; 0 = no limit
ip_group_requests_per_second = 1000   # shared by all peers of one IP group
ipv4_prefix = 24              # IP group: leading bits of the remote address
ipv6_prefix = 48
coarse_ipv4_prefix = 16       # validated; second request bucket at 10x rate planned
coarse_ipv6_prefix = 32
sender_puts_per_second = 10   # envelopes per sender key
strikes_to_ban = 100          # refused floods and bad signatures that ban a peer
ban_minutes = 10

[media]
shard_threshold = 1048576    # bytes; 1..1048576, larger uploads use Reed-Solomon

[maintenance]
interval_minutes = 60  # incremental vacuum, WAL checkpoint, PRAGMA optimize
vacuum_pages = 2048    # free 4 KiB pages released per store and run
io_mb_per_s = 16       # shared background rebuild writes in MiB/s; must be > 0

[network]
name = ""             # public; a name joins that network only, fixed per storage.dir
id_pow_bits = 22       # node-ID proof of work; lower it on test networks only
distinct_outbound_groups = true  # one routed peer per /16 (IPv6 /32) in each k-bucket
storage_trust_minutes = 60       # a new peer gets replicas after this; 0 on test networks
share_deny_list = false          # answer other nodes with the signed deny list (no notes)
accept_deny_lists = false        # fetch and store other nodes' lists; no action taken

[metrics]
listen = ""            # loopback ip:port such as 127.0.0.1:9464 serves GET /metrics; empty = off

[search]               # search role only
max_profile_bytes = 65536
max_bytes = 53687091200     # profiles-idx.db; above it the oldest heartbeats leave
max_scan_rows = 10000       # rows one query scans; past it the reply is partial
puzzle_wait_ms = 100        # median queue wait that turns the client puzzle on
max_puzzle_bits = 20        # puzzle difficulty ceiling, 12 to 32
topics = []                 # gossip topics 0..15 kept; empty = all 16, required on public
min_profile_pow_bits = 11   # Argon2id proof of work a profile needs to be indexed; 0..20
min_heartbeat_pow_bits = 8  # daily Argon2id proof a heartbeat needs; 0 = none, 0..16

[turn]                 # turn role only
urls = []              # e.g. "turn:turn.example.org:3478", "turns:turn.example.org:5349"
secret = ""            # coturn static-auth-secret; better DYAPPD__TURN__SECRET
credential_minutes = 60
```

Unknown keys are logged and ignored, so a config written for a newer node does not
stop an older one. At startup the node refuses to run as root, checks addresses,
roles and limits, and checks that every directory is writable. `dyappd keygen`
creates the node key (libp2p key, mode 0600) at `storage.key`, by default `node.key` in
`storage.dir`, writes `node.id` (its peer ID) there and prints the peer ID; `dyappd keygen -`
writes the key to stdout for [sealing](#node-key) and the peer ID to stderr. The key carries the node-ID proof of work: SHA-256 of
the peer ID starts with `network.id_pow_bits` zero bits, about a minute on 2 vCPU
([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)). Keygen refuses when a key or any
data already exists. Keygen, or the first start, also writes `network` with `network.name`; a
node never starts in a directory of another network, so moving a node to another network needs
a new `storage.dir` ([storage binding](../architecture/bootstrap.md#storage-network)). Turn `network.distinct_outbound_groups` off only on a network whose
nodes share a /16, such as a LAN or a test network. The node does not start without the key,
with a key that lacks the proof of work or with one that does not match `node.id`: a new key is
a new node, so delete the data to start from scratch. A key file (`storage.key` or
`storage.dir/node.key`) must be readable only by its owner: startup checks the opened file and
refuses any group/other permission bits with a `chmod 600` hint; it does not change the file's
permissions. Keygen creates mode 0600; owner-read-only mode 0400 also loads. A systemd
credential is not checked: systemd makes it readable to the service user only, through an ACL. Owned serialization/loading buffers are cleared on drop.
A key file that changes size during loading fails startup. Expired envelopes are deleted every
hour. Logging uses `RUST_LOG`
(e.g. `RUST_LOG=info`).

### Node key

`dyappd` reads the key from a file and does not care how the host protects it. With
`storage.key` empty it takes `$CREDENTIALS_DIRECTORY/node.key` when the unit passes that
credential, else `storage.dir/node.key`; a set `storage.key` always wins.

The package default seals the key with `systemd-creds` (systemd 252 on Debian 12), as in the
[quick start](#debian-12-package): the plain key never touches the disk, and the unit's
`LoadCredentialEncrypted=node.key` finds `/etc/credstore.encrypted/node.key` by name and
decrypts it into a memory-backed directory only the service reads. `--with-key=auto` binds the
credential to the VM's TPM 2.0 plus the host key in `/var/lib/systemd/credential.secret` when a
TPM exists (`systemd-creds has-tpm2`), so a copy of the disk alone does not open it. Without a
TPM only the host key is used, which sits on the same disk: that covers a stray copy of the key
file, not a copied volume. A sealed key cannot move to another host; to keep a backup, encrypt a
copy for yourself while generating, e.g. with `age`:

```bash
sudo -u dyappd dyappd keygen - --config /etc/dyappd.toml | tee >(age -r <your age key> >node.key.age) |
  sudo systemd-creds encrypt --with-key=auto --name=node.key - /etc/credstore.encrypted/node.key
```

To seal a plain key of an older install, stop the node, run
`sudo systemd-creds encrypt --with-key=auto --name=node.key /var/lib/dyappd/node.key /etc/credstore.encrypted/node.key`,
remove the plain file with `sudo shred -u /var/lib/dyappd/node.key` and start the node. While
both exist, the credential is used.

Without systemd or a TPM, choose one of:

- a plain 0600 file, `dyappd keygen` without `-` (containers, test nodes);
- a passphrase: keep the key encrypted (`age -p`) and before each start decrypt it into a 0600
  file on a `tmpfs` named by `storage.key`;
- an encrypted volume (LUKS, unlocked by Clevis/Tang or by hand) holding `storage.dir`.

### Development quick start

`test-peer` is the libp2p client CLI for scripts and manual checks
([commands](../architecture/bootstrap.md#test-client)).

**In Docker (no host Rust toolchain needed).** The network-test image builds `dyappd` and
`test-peer` (`docker/Dockerfile.network-test`):

```bash
make prepare            # builds the dev, UI-test and network-test images
make test-integration   # integration tests, three dyappd containers on an internal network,
                        # and a 5-minute memory plateau on release binaries

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

Nodes join through `seeds` and find each other over Kademlia. A client writes a profile or an
envelope to one node, which replicates it whole to 5 DHT points; nodes repair mailbox replicas between themselves
([replication and repair](../architecture/bootstrap.md#replication-and-repair)). Profile repair
and Reed-Solomon media up to 6 MiB are implemented. Owner-signed keeps trigger batched media
repair of missing items; no repair runs while the owner remains offline.

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

### Dependency list in the binary

Both `make deb` and `make image` build dyappd with `cargo auditable build` (cargo-auditable
0.7.6, installed in `docker/Dockerfile.deb`). The binary carries its crate names and versions as
zlib-compressed JSON in the ELF section `.dep-v0`, which `strip` keeps.
`scripts/check-auditable.py` checks the stripped binary that goes into the package and the image:
it fails unless the list names `dyapp-bootstrap` and every entry matches `Cargo.lock`. To check a
deployed binary against RustSec, run `cargo audit bin /usr/bin/dyappd` (cargo-audit) or extract
the list with `rust-audit-info`. Neither tool runs in this repository. The list names the crates
compiled in. It is not an SPDX SBOM and says nothing about whether an advisory's code is
reachable. Other platform binaries are not built this way yet.

## Client Configuration (planned)

No platform app connects to a bootstrap server yet, and there is no client API
for configuring bootstrap addresses. The intended behaviour is a
built-in list of bootstrap URLs plus a user-editable setting.

## Monitoring

### Health check (exists)

There is no HTTP health endpoint. A node is alive when it answers `/dyapp/node` `info`:

```bash
test-peer info /ip4/127.0.0.1/tcp/7070    # exit 0 and "STATUS_OK" → alive
```

The reply holds roles and limits only: no counts, peer numbers or replication fields.

### Metrics (exists)

Set `[metrics] listen = "127.0.0.1:9464"` (or `--metrics.listen`, `DYAPPD__METRICS__LISTEN`) to
serve Prometheus text at `http://127.0.0.1:9464/metrics`; only loopback addresses are accepted
and the node does not start when the port is taken. A scraper on another host reaches it through
a tunnel or a local agent. Families and listener limits:
[metric families](../architecture/bootstrap.md#metric-families),
[metrics listener](../architecture/bootstrap.md#metrics-listener).

```bash
curl -s http://127.0.0.1:9464/metrics | grep '^dyapp_connected_peers'
```

### Not implemented

These do not exist and need a separate implementation task before they can be
documented as runnable:

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
Background rebuild workers share `maintenance.io_mb_per_s` (default 16 MiB/s, positive),
waiting after every written batch outside the store lock. Each maintenance run reports active
rebuild files and percent complete as "store rebuild progress". Format rebuilds remain planned;
this budget does not change incremental vacuum's page limit or network repair's traffic cap.

### Backups

There is no online backup: copying the SQLite files while the server writes
to them does not give a consistent snapshot. Stop the process first.

`storage.dir` (`/var/lib/dyappd` from the package) holds `node.key` unless the key is
[sealed](#node-key) or set by `storage.key`, and `node.id`; the stores `profiles.db`, `messages.db` and `media.db`, with the media blob files in
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

# 2. Archive with a relative layout (top-level entry: bootstrap/); umask 077 keeps the
#    archive with node.key private
(umask 077; tar czf bootstrap-backup-$(date +%Y%m%d).tar.gz -C /tmp/ai bootstrap)

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

**Rolling update.** Profiles, mailboxes and media have five replicas on other
nodes ([replication](../architecture/bootstrap.md#replication-and-repair)), so
upgrade nodes one at a time: while one is down, clients read the other replicas,
and the owner's next visit repairs what it missed. A single node keeps the only
copy. On start a new `dyappd` upgrades its store files in place (each file keeps
its format in `PRAGMA user_version`); an older `dyappd` refuses files a newer one
has upgraded, so a downgrade needs the backup taken before the update. Building a
changed store format next to the old one while the old one keeps answering is
planned ([bootstrap design](../architecture/bootstrap.md#principles)).

**Verifying a release.** Releases are not published or signed yet. Planned: every artifact
carries a Sigstore keyless signature, SLSA build provenance and the SBOM, checked with
`gh attestation verify` before install ([supply chain](../security/threat-model.md#supply-chain)).

### Node key compromise

A copied `node.key` cannot be revoked, so the node starts over as a new node:

```bash
sudo systemctl stop dyappd
sudo rm -rf /var/lib/dyappd/*   # storage.dir: node.key, node.id and every store
sudo rm -f /etc/credstore.encrypted/node.key   # the sealed key, if used
sudo -u dyappd dyappd keygen - --config /etc/dyappd.toml |   # prints the new peer ID
  sudo systemd-creds encrypt --with-key=auto --name=node.key - /etc/credstore.encrypted/node.key
sudo systemctl start dyappd
```

Keygen refuses while any data is left, so the wipe comes first. Consequences:

- The node has a new peer ID. The stolen key can still run a node under the old one, so tell the
  operators who list the old ID (a seed `host:port/<peer ID>`, a deny entry) to replace it or
  deny it with `dyappd deny add <old peer ID>`.
- What the old node held is lost to it; replication repairs profiles, mailboxes and media from
  the other holders ([replication](../architecture/bootstrap.md#replication-and-repair)), and a
  single node loses its only copy.
- Update your own seeds and anything else that pins the old ID.

A dry run in the network-test image (keygen, serve, wipe, keygen, serve) gives a new peer ID and
the node answers `info`.

## Security

### Network

Every connection is libp2p: TCP with Noise and yamux, or QUIC (TLS 1.3), authenticated by the
node's libp2p key; there is no certificate authority.

### Clock

Retention, expiry and the 10-minute windows for signed times all use the node's clock. A clock
pushed forward, for example by a spoofed NTP server, makes the hourly cleanup delete envelopes,
profiles and attachments early; one pushed back refuses fresh heartbeats and keeps. Synchronise
the host with an authenticated source, such as `chrony` with NTS (`server <host> iburst nts`),
and watch for large steps in its log.

### Cloud hosting

On a rented VM, assume that the provider, or anyone holding your cloud account, can read the disk,
its snapshots and backups, write to the disk and read the VM's memory. They can then copy
`node.key` and the TURN secret, see what you see as an operator, and replace `dyappd` with a
backdoored build; they cannot forge users' signed records or read end-to-end content
([threat model](../security/threat-model.md#cloud-host-access)). A [sealed key](#node-key) bound
to a vTPM keeps a copied volume from yielding the node key; there is no encryption of the
stores at rest and no start-time integrity check, so:

- Protect the cloud account with hardware-key 2FA and limit who may snapshot or attach volumes.
- Keep backups that hold `node.key` off the provider, or encrypted with a key the provider does
  not have.
- If you suspect access, reinstall from a package you build or verify yourself, follow
  [Node key compromise](#node-key-compromise) and rotate the [TURN](#turn-relay) secret.

### Access Control

`max_connections_per_ip_group` (positive, default 32) bounds pending and established incoming
connections together, grouped by `ipv4_prefix` / `ipv6_prefix`. Outgoing connections are exempt.
Failures and closes free their slots. The limit remains active on LAN networks and its refusals
are reported as `group_refused` in the maintenance connection log.

The coarse prefixes (IPv4 0..32, IPv6 0..128; defaults /16 and /32) are accepted and validated,
but their second request bucket is still planned. Its rate will be ten times
`ip_group_requests_per_second`, bounded to 1..429496729 to prevent overflow.
Coarse and fine prefixes are independently configurable.

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
- The deny list is local to your node. Deny-list exchange (below) is not part of the first
  release; leave both switches off.
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
([resource guards](../architecture/bootstrap.md#resource-guards)). Several nodes survive the loss of
one for messages, profiles and media
([replication](../architecture/bootstrap.md#replication-and-repair)).

Sizing assumptions, unmeasured: with the default quotas a node needs about 16 GiB of disk
(`profiles_max_mb` 1 GiB + `messages_max_mb` 4 GiB + `media_max_mb` 10 GiB + `min_free_mb`
512 MiB; a node without the media role about 6 GiB; a search node adds `search.max_bytes`) and
1 GiB of memory (`MemoryMax=1G`,
`limits.max_memory_mb` 768). A full store answers `FULL` instead of growing; how many users
these numbers carry is not known until a load test exists.

## Troubleshooting

Only what applies to the code that exists today:

| Symptom | Check |
|---------|-------|
| `test-peer info` fails with a timeout or `request failed` | The node is down, the address is wrong (use `/ip4/`, not a host name) or a firewall blocks TCP/UDP 7070 |
| `dyappd` exits at startup about a directory | The storage directory is not writable by the node's user; fix permissions or set `DYAPPD__STORAGE__DIR` |
| `dyappd` exits with `storage belongs to ...` | `storage.dir` was bound to another `network.name`; restore that name or use a new directory |
| `test-peer info` fails against a named-network node | Set `DYAPPD__NETWORK__NAME` to the node's network name for `test-peer` too |
| `dyappd` exits with `has store format N, this dyappd knows up to M` | A newer `dyappd` upgraded that file; run that version or restore the backup taken before it ([Updates](#updates)) |
| `dyappd` exits with `has search index format N, this dyappd knows up to M` | A newer `dyappd` rebuilt the search index; run that version, or delete the named `profiles-idx-vN.db` to roll back: the index is disposable and catch-up refills it |
| `dyappd` exits with `search.lock: ...; another dyappd uses this search index` | Two nodes share one `storage.dir`; stop the other one or give each its own directory |
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

Planned work for the node is listed in
[Bootstrap: Limitations & Future](../architecture/bootstrap.md#limitations--future).

## References

- [Bootstrap Server Architecture](../architecture/bootstrap.md)
- [Testing & CI/CD](../testing/README.md)
