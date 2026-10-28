# Bootstrap node operator manual

This manual takes an operator from an empty Debian 12 host to a running `dyappd` bootstrap node
and through its daily operation. A Russian translation of the same revision is in
[bootstrap.ru.md](bootstrap.ru.md). How the node works inside is described in
[Bootstrap server architecture](../architecture/bootstrap.md); this manual links there where an
option needs more than a paragraph.

> ⚠️ **Not production-ready.** Clients built on `rust/client` send messages only as MLS
> ciphertext, padded to the next power of two from 1 KiB, so an operator sees envelope sizes and
> timing but not text; the node cannot check that a client encrypts. The node checks Ed25519 signatures: the owner's on profiles, the sender's on
> envelopes and the device's on mailbox reads. See
> [Encryption & Security Status](../security/encryption.md) and [Privacy](../security/privacy.md).

## Contents

1. [What a node does](#what-a-node-does)
2. [Requirements](#requirements)
3. [Getting dyappd](#getting-dyappd)
4. [Installing](#installing)
5. [Node key](#node-key)
6. [Configuration](#configuration)
7. [First start](#first-start)
8. [Status and metrics](#status-and-metrics)
9. [Operations](#operations)
10. [Backups](#backups)
11. [Security](#security)
12. [Capacity](#capacity)
13. [Troubleshooting](#troubleshooting)

## What a node does

`dyappd` is a libp2p node of the dyapp network. Clients write profiles, mailbox envelopes and
media to it; it keeps them for a limited time, replicates them to other nodes and serves them
back. A node serves one or more roles:

| Role | Serves |
|------|--------|
| `store` | Profiles, mailboxes and the network directory (Kademlia DHT). The base role; every other role needs it too. |
| `media` | Media blobs: avatars and chat attachments. |
| `turn` | Short-lived passwords for a coturn relay running next to the node ([TURN relay](#turn-relay)). Calls are not part of the first release. |
| `search` | Profile search ([Search](../architecture/bootstrap.md#search)). On by default, so a small network has search; it keeps an index of the whole network's profiles, up to `search.max_bytes` (50 GiB) of disk. Lower that on a small disk, or set `roles = ["store"]`. |
| `relay` | Short circuit relay v2 slots that let peers behind NAT reach each other and set up a hole punch, within the [`[relay]`](#relay) limits. Clients do not reserve slots yet. |

Every write is replicated whole to five DHT points and acknowledged once two distinct nodes
store it; media larger than 1 MiB is split into Reed-Solomon shards. Nodes repair missing
replicas between themselves ([replication](../architecture/bootstrap.md#replication-and-repair)).
A single node keeps the only copy, so a network needs several.

## Requirements

- Debian 12 (bookworm) with systemd, or any Linux host with Docker.
- About 2 vCPU, 1 GiB of memory and 16 GiB of disk with the default quotas
  ([Capacity](#capacity)). Run without swap: the key must not reach it.
- A public IPv4 and/or IPv6 address, with TCP **and** UDP open on the node port (33445 by default).
- An authenticated time source, such as `chrony` with NTS ([Clock](#clock)).
- Optional: a TPM 2.0 or a vTPM, so the sealed node key is bound to the machine
  ([Node key](#node-key)).

## Getting dyappd

A `vX.Y.Z` tag publishes the package, the tarball, an SPDX SBOM and `SHA256SUMS` to GitHub
Releases of `abbat/dyapp`, and the image as `ghcr.io/abbat/dyappd:X.Y.Z`
([ADR 0020](../decisions/0020-dyappd-release-channels.md)). Each file and the image carry a
Sigstore-signed SLSA build provenance attestation; check it before installing
([supply chain](../security/threat-model.md#supply-chain)):

```bash
sha256sum -c SHA256SUMS
gh attestation verify dyappd_X.Y.Z_amd64.deb --repo abbat/dyapp
gh attestation verify oci://ghcr.io/abbat/dyappd:X.Y.Z --repo abbat/dyapp
```

No release is published yet. Until then, or to check a release, build the package or the image
yourself from a commit you trust:

```bash
git clone https://github.com/abbat/dyapp && cd dyapp
make deb      # → build/deb/dyappd_<version>_<arch>.deb and a tarball, built in Debian 12
make image    # → the dyappd:latest container image
```

`make deb` builds a release binary in a Debian 12 image, so it runs on glibc 2.36, and
test-installs the package and the [tarball](#tarball-manual-install) on a clean
`debian:bookworm-slim`. The package's Debian `Description` stays in English: translations of that
field belong in repository `Translation-*` indexes rather than in a standalone `.deb`. Russian
operator help and diagnostics use the process locale. `make image` runs the image
read-only with no network, makes a key, starts the node, edits the deny list and checks that the
image has no shell. Both need only Docker on the build host.

**Dependency list in the binary.** Both builds use `cargo auditable build`: the binary carries
its crate names and versions as zlib-compressed JSON in the ELF section `.dep-v0`, which `strip`
keeps. `tools/scripts/check-auditable.py` checks that the list names `dyapp-bootstrap` and matches
`Cargo.lock`. To check an installed binary against RustSec, run `cargo audit bin /usr/bin/dyappd`
(cargo-audit) or extract the list with `rust-audit-info`. The list names the crates compiled in;
it is not an SPDX SBOM and says nothing about whether an advisory's code is reachable.

## Installing

### Debian 12 package

```bash
sudo apt install ./dyappd_*.deb        # creates the dyappd user, enables the unit
```

The package holds `/usr/bin/dyappd`, `/etc/dyappd.toml` (a conffile, kept on upgrade) and
`/lib/systemd/system/dyappd.service`. The unit:

- runs as the system user `dyappd` (no shell, no login) with no capabilities, `NoNewPrivileges`,
  a read-only system (`ProtectSystem=strict`), no access to `/home`, a private `/tmp` and write
  access to `/var/lib/dyappd` only;
- cannot replace `node.key` or `node.id` (read-only bind mounts), run anything outside `/usr`
  (`NoExecPaths=/`, `ExecPaths=/usr`), see other processes or the system part of `/proc`
  (`ProtectProc=invisible`, `ProcSubset=pid`), or share System V IPC and keyrings with other
  units (`PrivateIPC`, `RemoveIPC`, `KeyringMode=private`);
- cannot reach link-local and cloud metadata addresses
  (`IPAddressDeny=169.254.0.0/16 fe80::/10 fd00:ec2::254/128 100.100.100.200/32`), opens only
  IPv4, IPv6, Unix and netlink sockets (`RestrictAddressFamilies`) and counts its traffic
  (`IPAccounting=yes`: `systemctl show -p IPIngressBytes,IPEgressBytes dyappd`);
- is bounded to `MemoryHigh=768M`, `MemoryMax=1G`, `CPUQuota=150%` and `TasksMax=256`;
  `limits.max_memory_mb` defaults to MemoryHigh, so the node sheds requests before the kernel
  throttles it. On a larger host raise both in a drop-in;
- sets `LimitCORE=0` and `MemorySwapMax=0` (cgroup v2); `dyappd` also makes itself non-dumpable at
  start, so the node key stays out of core dumps and swap;
- decrypts the [sealed node key](#node-key) with `LoadCredentialEncrypted=node.key`;
- logs to the journal with `RUST_LOG=info`; `systemctl reload dyappd` sends SIGHUP with
  `/bin/kill`, hence the package's `procps` dependency.

The default port 33445 needs no privilege; any port works, seeds and clients carry it, and
another port alone does not hide the traffic from DPI
([why 33445](../architecture/bootstrap.md#default-port)). An explicitly configured 7070 address
keeps working; update firewalls and seed entries when moving an existing node. Another
`storage.dir` or a port below 1024 needs a drop-in (`sudo systemctl edit dyappd`):

```ini
[Service]
ReadWritePaths=/srv/dyappd
# Only for a port below 1024:
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
AmbientCapabilities=CAP_NET_BIND_SERVICE
```

The node resolves seed names itself, so a DNS resolver on a link-local address (some clouds
serve DNS at `169.254.169.253` or `fe80::…`; see `nameserver` in `/etc/resolv.conf`) is refused
by `IPAddressDeny` and seed names stop resolving. Give the node its own resolver list in a
drop-in:

```ini
[Service]
# /etc/dyappd/resolv.conf holds a nameserver outside 169.254.0.0/16 and fe80::/10
BindReadOnlyPaths=/etc/dyappd/resolv.conf:/etc/resolv.conf
```

Only when no other resolver is reachable, allow the one address with
`IPAddressAllow=169.254.169.253/32`: an allowed address wins over every deny, so never allow a
whole range. If the node logs `Address family not supported` after an upgrade, the new version
opens a socket family the unit does not list: add it to `RestrictAddressFamilies=` in a drop-in
and report it. `systemd-analyze security dyappd` lists what the sandbox still allows; read it as
a guide, its score has no threshold the package must meet.

Removing the package stops and disables the unit. Purging (`apt purge dyappd`) keeps the user,
`/var/lib/dyappd` and `/etc/credstore.encrypted/node.key`, because the key is the node's
identity: delete them by hand to [retire the node](#retiring-a-node).

### Container image

```bash
docker volume create dyappd
run="docker run --rm --read-only --cap-drop ALL --security-opt no-new-privileges:true -v dyappd:/var/lib/dyappd"
$run dyappd:latest keygen              # once: about a minute on 2 vCPU
$run -d --name dyappd --memory 1g -p 33445:33445/tcp -p 33445:33445/udp \
    -e DYAPPD_SEEDS='["seed.example.org:33445"]' dyappd:latest
docker exec dyappd dyappd deny add <entry> [note]   # applied within 10 s
docker exec dyappd dyappd status
docker logs dyappd
```

`tools/docker/Dockerfile.dyappd` copies the release binary with the glibc libraries it links onto an
empty base: no shell, no package manager, no `/etc`. The node runs as UID 65532 and writes only
`/var/lib/dyappd`, its `storage.dir` and the image's one volume, so the root can be read-only and
every capability dropped. A new named volume takes the owner and mode 0700 from the image; a
bind mount needs `chown 65532:65532`. The image sets `listen` to `["[::]:33445", "0.0.0.0:33445"]`;
other settings are `DYAPPD_` variables, `--<section>.<key>` options after the image name, or a
config file mounted read-only at `/etc/dyappd.toml`, which is read without `--config`. With
`--memory 1g` the default `limits.max_memory_mb = 768` sheds requests before Docker's limit. In
a container the key is a plain 0600 file in the volume.

The image is not published to a registry, and there are no Kubernetes manifests yet.

### Tarball (manual install)

For a host without `apt`, or an operator who wants to see every step: `make deb` also writes
`build/deb/dyappd-<version>-<machine>-linux-gnu.tar.gz` with the same binary (glibc 2.36 or
newer), `dyappd.toml`, `LICENSE` and `dyappd.service`, the package's unit with the binary at
`/usr/local/bin/dyappd`. It is gzip rather than xz: `tar -xzf` works everywhere and the archive
is a few megabytes either way. `make deb` installs it this way on a clean Debian 12:

```bash
tar -xzf dyappd-*-linux-gnu.tar.gz && cd dyappd-*-linux-gnu
sudo install -m 0755 dyappd /usr/local/bin/dyappd
sudo install -m 0644 dyappd.toml /etc/dyappd.toml
sudo install -m 0644 dyappd.service /etc/systemd/system/dyappd.service
sudo useradd --system --user-group --home-dir /var/lib/dyappd --shell /usr/sbin/nologin dyappd
sudo install -d -o dyappd -g dyappd -m 0750 /var/lib/dyappd
sudo systemctl daemon-reload
```

Then make and seal the key ([Node key](#node-key)), set `listen` in `/etc/dyappd.toml`
([Configuration](#configuration)) and start: `sudo systemctl enable --now dyappd`;
`sudo -u dyappd dyappd status` prints `node running`. The unit is the package's, with the same
sandbox and limits, so the rest of this manual applies as written.

It is a system unit with `User=dyappd`, not a `systemd --user` unit: a user manager runs only
while its user is logged in unless lingering is enabled, and it can neither apply the system
sandbox above nor decrypt a credential sealed with the host key. Without systemd, run
`dyappd --config /etc/dyappd.toml` as `dyappd` under your supervisor with the key as a 0600 file
(`storage.key`); `dyappd status` exits 0 while the node answers and 3 when it does not.

To update, keep the old binary (`sudo cp -f /usr/local/bin/dyappd /usr/local/bin/dyappd.previous`),
install the new one and the new unit, compare `dyappd.toml` with your `/etc/dyappd.toml`, then
`sudo systemctl daemon-reload && sudo systemctl restart dyappd` ([Updates](#updates)). To roll
back, put `dyappd.previous` back and restart ([Rollback](#rollback)). To remove, disable the
unit and delete the three installed files; the user, `/var/lib/dyappd` and the sealed key stay
as with the package.

## Node key

The node key is a libp2p Ed25519 key. Its peer ID is the node's identity: peers pin it, the
stores are bound to it, and a new key is a new node. The key carries a proof of work: SHA-256 of
the peer ID starts with `network.id_pow_bits` zero bits (22 on the public network, about a
minute on 2 vCPU; [ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)).

`dyappd` reads the key from a file and does not care how the host protects it. With
`storage.key` empty it takes `$CREDENTIALS_DIRECTORY/node.key` when the unit passes that
credential, else `storage.dir/node.key`; a set `storage.key` always wins.

**Sealed with systemd-creds (package default).** The plain key never touches the disk:

```bash
sudo install -d -m 0700 /etc/credstore.encrypted
sudo -u dyappd dyappd keygen - |   # prints the peer ID on stderr
  sudo systemd-creds encrypt --with-key=auto --name=node.key - /etc/credstore.encrypted/node.key
```

The unit's `LoadCredentialEncrypted=node.key` finds the file by name and decrypts it into a
memory-backed directory only the service reads. `--with-key=auto` binds the credential to the
TPM 2.0 plus the host key in `/var/lib/systemd/credential.secret` when a TPM exists
(`systemd-creds has-tpm2`), so a copy of the disk alone does not open it. Without a TPM only the
host key is used, which sits on the same disk: that covers a stray copy of the key file, not a
copied volume.

**Checking the TPM.** A TPM device on the host does not prove the key is bound to it; check each
step. These steps follow the systemd documentation and have not yet been run on a real TPM or
vTPM; report any step that behaves differently.

| Step | Command | Expected |
|------|---------|----------|
| 1. Device exposed | `ls -l /dev/tpmrm0` | The file exists. If not: on hardware, enable TPM 2.0 in the firmware; on a VM, enable the provider's vTPM (e.g. GCP Shielded VM, Azure Trusted Launch, AWS NitroTPM), which often needs a new VM. Containers do not get a TPM: there the key is a plain 0600 file. |
| 2. systemd can use it | `systemd-creds has-tpm2` | `yes` and exit code 0. `partial` names the missing part (`-firmware`, `-driver`, `-system`). |
| 3. Seal to it | `--with-key=host+tpm2` instead of `--with-key=auto` in the keygen command above | The command succeeds. Unlike `auto`, it fails instead of falling back to the host key alone. |
| 4. The node opens it | `sudo systemctl restart dyappd`, then `sudo -u dyappd dyappd status` | `node running`. A credential bound to a TPM that changed or was cleared fails to decrypt, and the unit fails to start. |

Without a TPM (step 1 or 2 fails), seal with `--with-key=host`: the key is still never stored in
plain text, but anyone with a copy of the whole disk can open it. Use a passphrase or an
encrypted volume (below) if that matters. A TPM reset or a move to another VM makes the sealed
key unreadable: keep the encrypted backup copy from the next paragraph.

**Keeping a copy.** A sealed key cannot move to another host. To keep a backup, encrypt a copy
for yourself while generating, e.g. with `age`:

```bash
sudo -u dyappd dyappd keygen - | tee >(age -r <your age key> >node.key.age) |
  sudo systemd-creds encrypt --with-key=auto --name=node.key - /etc/credstore.encrypted/node.key
```

**Sealing an older plain key.** Stop the node, run
`sudo systemd-creds encrypt --with-key=auto --name=node.key /var/lib/dyappd/node.key /etc/credstore.encrypted/node.key`,
remove the plain file with `sudo shred -u /var/lib/dyappd/node.key` and start the node. While
both exist, the credential is used.

**Without systemd or a TPM**, choose one of:

- a plain 0600 file, `dyappd keygen` without `-` (containers, test nodes);
- a passphrase: keep the key encrypted (`age -p`) and before each start decrypt it into a 0600
  file on a `tmpfs` named by `storage.key`;
- an encrypted volume (LUKS, unlocked by Clevis/Tang or by hand) holding `storage.dir`.

**Peer ID.** `sudo -u dyappd dyappd id` (in Docker: `docker exec dyappd dyappd id`) prints the
peer ID, then one `seeds` entry per `external` address, ready to hand to the operators who list
this node:

```text
12D3KooW...
[2001:db8::7]:33445/12D3KooW...
192.0.2.7:33445/12D3KooW...
```

What keygen and startup check:

- `dyappd keygen` writes the key (mode 0600) to `storage.key` or `storage.dir/node.key`, writes
  `node.id` (the peer ID) and `network` (the network name) to `storage.dir` and prints the peer
  ID; `dyappd keygen -` writes the key to stdout, refuses a terminal, and prints the peer ID on
  stderr.
- Keygen refuses when a key or any data already exists.
- The node does not start without the key, with a key that lacks the proof of work or with one
  that does not match `node.id`. `dyappd probe-key --storage.dir=<stopped-copy>` performs the
  same key/ID check without opening a store or listener and prints the verified peer ID. Use
  `--storage.key=<temporary-key-file>` when restoring an external or decrypted backup key;
  run as the copy's owner with the original `network.id_pow_bits` setting. This command checks
  identity only; the backup inventory and SQLite checks run separately.
- A key file must be readable only by its owner: startup refuses group or other permission bits
  with a `chmod 600` hint and never changes the mode itself; 0400 also loads. A systemd
  credential is not checked: systemd makes it readable to the service user only.

## Configuration

### Where settings come from

Every setting is optional. Later sources override earlier ones:

1. the defaults in the code;
2. the TOML file named with `--config`, else the first of `./dyappd.toml`, `~/dyappd.toml` and
   `/etc/dyappd.toml` that exists (`/etc/dyappd.toml` in the package, every key commented
   out with its default, [`rust/bootstrap/dyappd.toml`](../../rust/bootstrap/dyappd.toml) in
   the repository; a test keeps that file equal to the code);
3. environment variables `DYAPPD_<SECTION>_<KEY>`, parsed as TOML (a bare string is taken as
   is), e.g. `DYAPPD_LIMITS_MESSAGE_TTL_HOURS=48` or `DYAPPD_LISTEN='["[2001:db8::7]:33445"]'`;
4. options `--<section>.<key>` with `-` for `_`, e.g. `--limits.max-connections 500` or
   `--storage.dir=/srv/dyappd`; a list option is given once per item and replaces the list
   (`--listen '[::]:33445' --listen 0.0.0.0:33445`).

`dyappd --help` lists every option with its default. With a Russian message locale (`LC_ALL`,
then `LC_MESSAGES`, then `LANG`), command usage and config-loading instructions appear in Russian;
command and option names and default values stay unchanged, while option descriptions are
translated. A parity test requires a Russian description for every new option. Project-authored
option, seed, and TURN errors are translated. Config file read failures and TOML syntax errors
now have Russian guidance while preserving paths, positions and original technical causes.
Syntax and type errors never echo source values that may contain secrets. Known type mismatches
name the config key and expected TOML type; other deserialization failures use a generic safe
message. Missing TURN secret files also get Russian guidance. See the
[diagnostic design](../design/localized-config-diagnostics.md). An unknown option stops the node; an
unknown key in the file or the environment is logged and ignored, so a config written for a
newer node does not stop an older one. Changes take effect on restart
(`sudo systemctl restart dyappd`); only the deny list is reloaded while running. Keep
`turn.secret` off the command line: other users see it in `ps`.

At startup the node refuses to run as root, checks addresses, roles and limits against the
ranges below, creates missing directories and checks that each is writable. Commands other
than `keygen` must run as the owner of `storage.dir` (`sudo -u dyappd dyappd ...`).

### Addresses and roles

| Key | Default | Meaning |
|-----|---------|---------|
| `listen` | `["[::1]:33445", "127.0.0.1:33445"]` | Addresses the node listens on. Loopback only by default, so a fresh install serves nobody; a serving node sets `["[::]:33445", "0.0.0.0:33445"]`. IPv6 sockets are IPv6-only, so a dual-stack node lists both. Each address serves TCP and QUIC (UDP) on its port. An address that does not open is logged as `not listening` and skipped; the node exits only when none opens. |
| `external` | `[]` | Public addresses announced to peers, when known (a static address, port forwarding). Empty: peers learn the listen addresses through identify (only the global ones when there are any), and the address they see the node from is announced once AutoNAT confirms it. Until then the node is a DHT client: it uses the DHT but serves no part of it. |
| `seeds` | `[]` | Nodes to join through: `host:port` or `host:port/<peer ID>`, the host an IP or a DNS name (e.g. `seed.example.org:33445`, `[2001:db8::7]:33445`). The peer ID, printed by the seed's `dyappd id`, pins the seed's key: a different key is refused and logged at warn. A name with several A/AAAA records is tried address by address. Without a pin, whoever controls the name or host controls the first peers. Later starts also dial the peers cached in `storage.dir`, so seeds are only a way in. Libp2p multiaddrs are refused. |
| `roles` | `["store", "search"]` | Any of `store`, `media`, `turn`, `search`, `relay` ([What a node does](#what-a-node-does)). Each non-store role is refused at startup without `store` in the list. A node without a role answers its requests `UNSUPPORTED`. |

`listen` and `external` take an IP, not a host name; an IPv6 address goes in brackets.

<a id="without-a-domain"></a>**Without a domain.** A node needs no domain of its own. A seed
entry names the node by address or by name, and the peer ID after `/` is what proves it is your
node; the name only finds it. Choose by how stable the address is:

| Choice | Example `seeds` entry | Pros | Cons |
|--------|-----------------------|------|------|
| Direct IP | `192.0.2.7:33445/12D3KooW...` | No third party; nothing to renew | Every operator who lists the node edits the entry when the address changes |
| Free dynamic DNS name | `mynode.duckdns.org:33445/12D3KooW...` | The entry survives an address change; free | A provider outage or a lost name stops new nodes from joining through this seed; an update token to protect |
| Owned domain | `seed1.example.org:33445/12D3KooW...` | Under your control; your own DNS provider and TTL | Costs money and renewal; lost on expiry like any name |

Free dynamic DNS providers, as their own pages state (October 2026; terms change, check before
use):

| Provider | Free names | IPv6 (AAAA) | Update |
|----------|-----------|-------------|--------|
| [Duck DNS](https://www.duckdns.org/spec.jsp) | a few `*.duckdns.org` names | Yes, given explicitly (not detected) | HTTPS GET with a token: `?domains=…&token=…&ip=…&ipv6=…` |
| [Dynu](https://www.dynu.com/FAQ/Dynamic-DNS-Service) | 4 hostnames | Yes | Update client or HTTPS API |
| [FreeDNS (afraid.org)](https://freedns.afraid.org/faq/) | a few subdomains of shared domains | Yes | Update URL; linked records sharing an IP update together; TTL 60 s |
| [ClouDNS](https://www.cloudns.net/dynamic-dns/lang/en/) | 1 dynamic hostname | Yes | One update URL per A or AAAA record |

Always pin the peer ID in a seed that names a third-party host: whoever holds the name or the
account can point it anywhere, and a pin turns that into a refused dial instead of a takeover.
Keep the update token out of shared configs; a stolen token can repoint the name, which a pin
again turns into an outage, not an attack. List more than one seed, ideally with different
providers or a direct IP, so one provider's outage or a lapsed name does not lock new nodes out.
A provider sees who resolves the name and when. A node resolves a name on every dial, so a
changed address is used after the DNS TTL, and a running node keeps its peer cache either way.

### `[storage]`

| Key | Default | Meaning |
|-----|---------|---------|
| `dir` | `/var/lib/dyappd` | All node data ([Backups](#backups) lists the files). Another path needs `ReadWritePaths=` in a unit drop-in. One directory belongs to one node and one network. |
| `key` | `""` | The node key file. Empty: the unit's `node.key` credential if there is one, else `dir/node.key` ([Node key](#node-key)). |

### `[limits]`

Retention:

| Key | Default | Meaning |
|-----|---------|---------|
| `message_ttl_hours` | 24 | How long an undelivered mailbox envelope is kept. |
| `attachment_retention_hours` | 168 | How long a chat attachment the recipient never released is kept. |
| `media_cache_minutes` | 10 | How long a blob assembled from shards for piece-by-piece reads stays cached after its last read. |
| `profile_ttl_days` | 30 | How long a profile or tombstone, and the owner's media, is kept after the owner's last signed action. |

Disk. A full store answers `FULL` to writes instead of growing; disk use stays below the sum of
the store sizes plus SQLite overhead:

| Key | Default | Meaning |
|-----|---------|---------|
| `profiles_max_mb` | 1024 | Profile store size. |
| `messages_max_mb` | 4096 | Mailbox store size. |
| `signal_max_kb` | 2048 | Signals (likes, views) kept per mailbox, apart from the messages' 10 MiB; at the quota the oldest are deleted. At least 2. |
| `require_write_tokens` | `true` | Messages without a mailbox write token go to a small per-mailbox pool instead of the 10 MiB mailbox quota ([write tokens](../architecture/bootstrap.md#mailbox-envelope)). |
| `mailbox_pool_kb` | 1024 | That pool, per mailbox, while `require_write_tokens` is on; when full, the oldest envelope of the sender group holding most bytes is deleted. At least 101. |
| `tokenless_share_pct` | 20 | Tokenless envelope bytes kept store-wide, in percent of `messages_max_mb`. |
| `media_max_mb` | 10240 | Media store size (media role); when full, cached assembled blobs go first, then unreleased attachments, then the oldest owners, down to 95 %. |
| `media_per_owner_mb` | 10 | Media bytes of one owner, billed as whole copies × 5 and shards as `ceil(bytes × (K+4)/K)`. |
| `min_free_mb` | 512 | Free space kept on the file system of `storage.dir`; writes stop below it. |

Rates. A peer is a libp2p peer ID; an IP group is the addresses sharing the leading
`ipv4_prefix` / `ipv6_prefix` bits:

| Key | Default | Meaning |
|-----|---------|---------|
| `requests_per_second` | 100 | Profile and mailbox requests per second from one peer. |
| `media_requests_per_second` | 10 | Media requests per second from one peer. |
| `ip_group_requests_per_second` | 1000 | Requests per second shared by all peers of one IP group; 1..100000000. |
| `sender_puts_per_second` | 10 | Envelopes per second accepted from one sender key. |
| `bytes_per_second` | 0 | Node protocol bytes in and out per second. Media and repair are shed from 75 %, profiles from 90 %, the mailbox at 100 %. 0 = no limit. |
| `ipv4_prefix` | 24 | Leading bits of an IPv4 address that form its group. |
| `ipv6_prefix` | 48 | Leading bits of an IPv6 address that form its group. |
| `coarse_ipv4_prefix` | 16 | Leading bits of an IPv4 address that form its coarse group (0..32); all its peers share a second budget of ten times `ip_group_requests_per_second`. |
| `coarse_ipv6_prefix` | 32 | The same for IPv6 (0..128). |

Connections, memory and bans:

| Key | Default | Meaning |
|-----|---------|---------|
| `max_connections` | 1000 | Connections in total. |
| `max_connections_per_peer` | 4 | Connections from one peer. |
| `max_connections_per_ip_group` | 32 | Pending and established incoming connections per IP group; outgoing connections are exempt. Refusals are counted as `group_refused` in the connection log. |
| `max_streams` | 16 | Concurrent streams per connection and protocol. |
| `max_memory_mb` | 768 | Resident memory of the process: from 80 % requests that the byte rate sheds are refused, at 100 % every request is dropped and new connections refused. 0 = no limit. Keep it at the unit's `MemoryHigh`. |
| `strikes_to_ban` | 100 | Refused floods and bad signatures that ban a peer. |
| `ban_minutes` | 10 | How long a local ban lasts. |

Details: [resource guards](../architecture/bootstrap.md#resource-guards) and
[rate limiting](../architecture/bootstrap.md#rate-limiting).

### `[network]`

| Key | Default | Meaning |
|-----|---------|---------|
| `name` | `""` | The network to join; empty is the public network. A named network is public too (anyone who knows the name can join) and keeps its nodes apart from other networks. Keygen or the first start writes it to `storage.dir/network`, and a node never starts in a directory of another network ([storage binding](../architecture/bootstrap.md#storage-network)). |
| `id_pow_bits` | 22 | Proof of work in node keys, required of this node and of every routed peer. Nodes of the public network do not route a key with fewer bits: lower it on test networks only. |
| `distinct_outbound_groups` | `true` | At most one routed peer per /16 (IPv6 /32) in each k-bucket, an eclipse defence. Turn it off only on a network whose nodes share a /16, such as a LAN or a test network. |
| `guard_dials` | `true` | Refuse outbound dials to private and reserved IPs, also after DNS, and cap concurrent dials per /24 (IPv6 /48). Turn it off only on a network whose nodes dial each other on private or loopback addresses. |
| `deny_dial` | cloud metadata and link-local ranges | CIDR ranges never dialed, also after DNS, even with `guard_dials = false`: `169.254.0.0/16`, `fe80::/10`, `fd00:ec2::254/128`, `100.100.100.200/32`. `[]` turns the filter off; an invalid entry stops startup. The status line counts refusals as `denied_dials`. |
| `storage_trust_minutes` | 60 | Minutes after the first connection before a peer gets replicas; 0 trusts at once (test networks). The time is kept in memory only: see [First deployment](#first-deployment). |
| `share_deny_list` | `false` | Answer other nodes' requests with this node's signed deny list (without notes). Not part of the first release; leave it off. |
| `accept_deny_lists` | `false` | Fetch connected nodes' deny lists each maintenance run and store them; no action is taken on them. Not part of the first release; leave it off. |

### `[maintenance]`

| Key | Default | Meaning |
|-----|---------|---------|
| `interval_minutes` | 60 | Minutes between runs: expired data cleanup, incremental vacuum, WAL checkpoint, `PRAGMA optimize`, peer list save, seed redial. |
| `vacuum_pages` | 2048 | Free 4 KiB pages released per store and run, the search index included; bounds the I/O of one run. |
| `io_mb_per_s` | 16 | Write budget in MiB/s shared by background store rebuilds; must be above 0. |

### `[metrics]`

| Key | Default | Meaning |
|-----|---------|---------|
| `port` | `0` | A port, such as `9464`, serving Prometheus text at `GET /metrics` on `[::1]` and `127.0.0.1` without authentication. 0: off. Only these loopback addresses are bound; a host without IPv6 serves `127.0.0.1` alone, and the node does not start when the port is taken. |

### `[media]`

| Key | Default | Meaning |
|-----|---------|---------|
| `shard_threshold` | 1048576 | Bytes; uploads above it are split into Reed-Solomon shards (K ≤ 6 data, 4 parity), smaller ones are stored whole. 1..1048576 ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)). |

### `[turn]`

Used by the `turn` role only.

| Key | Default | Meaning |
|-----|---------|---------|
| `urls` | `[]` | `turn:` / `turns:` URLs of the coturn relay handed to peers, e.g. `turn:turn.example.org:3478`. |
| `secret` | `""` | coturn's `static-auth-secret`. Prefer `secret_file`; otherwise set `DYAPPD_TURN_SECRET` in a mode 0600 `EnvironmentFile=`, never on the command line. |
| `secret_file` | `""` | A file holding the secret, e.g. a systemd credential (`LoadCredential=turn.secret:/etc/dyappd/turn.secret` and `DYAPPD_TURN_SECRET_FILE=%d/turn.secret`). One trailing newline is dropped. Setting both `secret` and `secret_file`, or an empty or unreadable file, stops startup. |
| `credential_minutes` | 60 | Lifetime of the passwords handed out. |

### `[relay]`

Used by the `relay` role only. A circuit carries a peer's traffic through this node only until
the peers punch a direct connection, so the limits are small. libp2p's own rate limits per
peer and per IP apply on top. Every value must be at least 1
([ADR 0031](../decisions/0031-circuit-relay-and-hole-punching.md)).

| Key | Default | Meaning |
|-----|---------|---------|
| `max_reservations` | 128 | Reservations held at once. |
| `max_reservations_per_peer` | 4 | Reservations of one peer. |
| `reservation_minutes` | 60 | How long a reservation lasts before the peer renews it. |
| `max_circuits` | 16 | Circuits open at once. |
| `max_circuits_per_peer` | 4 | Circuits to or from one peer. |
| `circuit_seconds` | 120 | How long one circuit stays open. |
| `circuit_kb` | 128 | KiB one circuit carries each way before it closes. |

### `[search]`

Used by the `search` role only. Details: [Search](../architecture/bootstrap.md#search).

| Key | Default | Meaning |
|-----|---------|---------|
| `max_profile_bytes` | 65536 | Largest profile record indexed, in bytes, reported to clients in `info`; 1..65536. |
| `max_bytes` | 53687091200 | Size of `profiles-idx.db` (records plus index); above it established keys leave, from the place with the most profiles first. |
| `newcomer_days` | 30 | Days a key counts as a newcomer after its first put; 1..365. |
| `newcomer_share` | 0.1 | Share of `max_bytes` newcomers' records may hold together, above 0 and at most 1; past it the newcomer with the oldest heartbeat leaves. |
| `max_scan_rows` | 10000 | Rows one query scans at most; past it the reply is marked partial. |
| `puzzle_wait_ms` | 100 | Median queue wait, in ms, above which queries need a client hash puzzle; at least 1. |
| `max_puzzle_bits` | 20 | Puzzle difficulty ceiling, 12..32. |
| `topics` | `[]` | Gossip topics 0..15 kept; empty keeps all 16. A subset is refused on the public network. |
| `min_profile_pow_bits` | 11 | Shared store/search Argon2id profile minimum, 0..20. Store and search enforce it; store nodes advertise it in `info`. Set 0 explicitly on every node of a private test network if needed. |
| `min_heartbeat_pow_bits` | 8 | Daily Argon2id proof a heartbeat needs; 0 = none, 0..16. |

## First start

On the package install, after [sealing the key](#node-key):

```bash
sudoedit /etc/dyappd.toml        # at least: listen, seeds (or external on a known address)
sudo systemctl start dyappd
journalctl -u dyappd -f          # "listening on ..." for every address
sudo -u dyappd dyappd status
```

A minimal serving config:

```toml
listen = ["[::]:33445", "0.0.0.0:33445"]
seeds = ["seed1.example.org:33445/12D3KooW...", "seed2.example.org:33445/12D3KooW..."]
```

Open TCP and UDP on the port in the host and cloud firewalls. Check from another host with
`dyappd info` ([client commands](../architecture/bootstrap.md#client-commands)):

```bash
dyappd info node.example.org:33445   # exit 0 and "STATUS_OK" → alive
dyappd info [2001:db8::7]:33445      # an IPv6 address goes in brackets
dyappd info 192.0.2.7:33445          # an IPv4 address
```

A new node answers `STATUS_FULL` to writes until it has trusted peers for
`network.storage_trust_minutes`; reads work at once.

### First deployment

The steps below start a network of seed nodes with the production defaults
(`id_pow_bits = 22`, `distinct_outbound_groups = true`, `storage_trust_minutes = 60`) and take it
through one upgrade and one rollback.

A node sends replicas only to peers it first connected to at least
`network.storage_trust_minutes` ago, and keeps that time in memory only. A put (a profile, an
envelope, a media part) needs a second holder, so a node that trusts no peer yet answers it
`STATUS_FULL` after 10 seconds; reads are not affected. A new network therefore refuses all
writes for the first trust period, and every restarted node refuses writes through it for one
period. The peers of a restarted node keep trusting it while it stays in their routing table.

1. **Cold start.** Generate each key (about a minute per node at 22 bits), list the other seeds
   in `seeds` on every seed and start them in any order; a seed that starts first is dialled by
   the later ones. Check each with `dyappd info`.
2. **Canary.** After `storage_trust_minutes`, publish a test profile through each node:
   `dyappd publish` returns `STATUS_OK` and `dyappd get` finds the record on at least two
   nodes. Before that it returns `STATUS_FULL`.
3. **Rolling upgrade.** For each node in turn: stop it, take a [backup](#backups) of
   `storage.dir`, start the new version, check its health and that it still serves the canary
   records, publish a canary through a node that was not restarted in the last trust period,
   then wait `storage_trust_minutes` before the next node. Restarting every node within one
   period stops all writes until the first restarted node's period ends.
4. **Rollback.** Stop the node and start the old version. When the update changed a store
   format, the old version exits with "has store format N, this dyappd knows up to M"; restore
   the backup from step 3 and start the old version on it. Writes the node took after the backup
   are lost on that node; the other replicas and the owner's next visit repair them. There is no
   rollback across a store-format change without that backup.

`make rehearsal FROM=<git ref>` runs these steps on three loopback nodes (127.1.0.1, 127.2.0.1
and 127.3.0.1, three /16 groups) with release builds of the ref and of the working tree, the ref
being a commit before a store-format step; it takes about three trust periods
(`tests/scripts/rehearsal.py`). The manual `rehearsal.yml` workflow runs the same check on CI with the
ref as its `from` input.

## Status and metrics

**Status.** `sudo -u dyappd dyappd status` (in Docker:
`docker exec dyappd dyappd status`) prints one `name value` line each, most important first.
Field names and the `running`, `stopped`, `none`, and `unavailable` values are stable across
locales because scripts parse them. With a Russian locale, operator errors and `deny add/remove`
confirmations are translated. Client command argument and connection errors are translated too;
protocol status values in JSON stay unchanged. Invalid-address guidance uses the default port 33445.

| Line | Value |
|---|---|
| `node` | `running` when the node answers `info` on a `listen` address (a `[::]` or `0.0.0.0` host is dialed on loopback, 15 s at most), else `stopped` |
| `peer_id` | the node's peer ID, empty before `keygen` |
| `profiles`, `envelopes`, `media_blobs`, `deny_entries` | records stored; `none` when that store was never created |
| `purged_blobs` | cumulative local media blobs purged by the deny list; `none` before the media store exists |
| `envelope_bytes`, `media_bytes` | stored bytes; `none` likewise |
| `cached_peers` | peers saved for the next start |
| `uptime_seconds`, `requests`, `wire_in_bytes`, `wire_out_bytes` | seconds since the node started, then requests and payload bytes received and sent over all protocols since then, from the [metrics](#metrics) listener; `unavailable` when the node is stopped, `[metrics] port` is 0 or the listener does not answer |

It exits 0 when the node runs and 3 (the LSB "not running" code) when it does not, so
`dyappd status >/dev/null || alert` works without systemd. It opens the stores read-only, does
not scan beyond SQLite counts and works while the node runs. There is no admin HTTP API.

**Logs.** `journalctl -u dyappd` (run by hand: stderr). The node logs startup, listen addresses,
storage errors, the hourly cleanup and maintenance; not individual requests. `RUST_LOG` sets the
level, e.g. `RUST_LOG=info` (the unit's default) or `RUST_LOG=debug`.
`sudo systemctl reload dyappd` (SIGHUP) logs one in-memory state line: pending requests,
connection and peer counts.

**Health.** There is no HTTP health endpoint. A node is alive when it answers `/dyapp/node`
`info` (`dyappd info`, above; `dyappd status` checks this locally); the reply holds roles and
limits only.

### Metrics

Set `[metrics] port = 9464` to serve Prometheus text at `http://[::1]:9464/metrics` and,
for IPv4-only tools, `http://127.0.0.1:9464/metrics`. A scraper on another host reaches it through a tunnel or a local
agent. Families and listener limits:
[metric families](../architecture/bootstrap.md#metric-families),
[metrics listener](../architecture/bootstrap.md#metrics-listener).

```bash
curl -s 'http://[::1]:9464/metrics' | grep '^dyapp_connected_peers'
```

Reading the values:

- Counters (`*_total`) start at 0 with the process. A restart shows as a new
  `dyapp_process_start_time_seconds`; use `rate()`/`increase()` in Prometheus or a "Change per
  second" step in Zabbix, both of which treat a drop as a reset.
- Storage gauges are up to 5 minutes old: alert on
  `time() - dyapp_snapshot_timestamp_seconds > 900` (no snapshot for three periods) and on
  `dyapp_storage_read_ok == 0`, not on a missing `storage_used_bytes`.
- The series set is fixed by the roles and configured stores.

**Prometheus.** Run Prometheus, or an agent that forwards to it, on the node host:

```yaml
scrape_configs:
  - job_name: dyappd
    scrape_interval: 60s
    scrape_timeout: 4s     # under the listener's 5 s deadline
    static_configs:
      - targets: ['[::1]:9464']
```

**Zabbix.** One master item fetches the page and dependent items cut samples out of it with the
"Prometheus pattern" preprocessing step, so each scrape is one request:

- Master item, type "HTTP agent", URL `http://[::1]:9464/metrics`, type of information
  "Text", history 0. The HTTP agent runs on the Zabbix server or proxy, so this needs one on the
  node host. Otherwise use a Zabbix agent item `web.page.get[127.0.0.1,/metrics,9464]` as the
  master; it returns the response headers too, so add a first "Regular expression" step
  `\r\n\r\n([\s\S]*)` with output `\1`.
- Dependent items, master the item above, preprocessing "Prometheus pattern", for example
  `dyapp_connected_peers` (numeric unsigned), or
  `dyapp_request_outcomes_total{protocol="mailbox",outcome="omitted"}` followed by "Change per
  second".

Neither setup is installed or tested by this project; the exposition itself is checked by the
`metrics` unit tests.

## Operations

### Config changes

Edit `/etc/dyappd.toml` (or a drop-in's `Environment=`) and `sudo systemctl restart dyappd`. A
restart makes the node refuse writes for `storage_trust_minutes`; on a network of several nodes
restart them one at a time as in a [rolling upgrade](#updates).

### Updates

[ADR 0023](../decisions/0023-bootstrap-node-update-policy.md) approves an
operator opt-in unattended updater for v0.0.1, but no updater or trusted
automatic package channel exists yet. Until its release, backup, rollout and
recovery gates pass, upgrade manually with the procedure below. Do not enable
an unattended package upgrade on this node as a substitute: it cannot account
for the trust window or store migration. A node operator will have to opt in
explicitly when the tested updater becomes available.

Install the new package (`sudo apt install ./dyappd_<new>.deb`) or start the new image, one node
at a time:

- Profiles, mailboxes and media have five replicas on other nodes, so while one node is down
  clients read the other replicas and the owner's next visit repairs what it missed. A single
  node keeps the only copy.
- A restarted node trusts no peer for `network.storage_trust_minutes` and refuses writes for
  that long; restart the next node only after it ([First deployment](#first-deployment)).
- On start a new `dyappd` upgrades its store files in place (each keeps its format in
  `PRAGMA user_version`). An older `dyappd` refuses files a newer one has upgraded, so a
  downgrade needs the [backup](#backups) taken before the update. Building a changed format next
  to the old one while the old one keeps answering is planned.
- `apt` keeps your edited `/etc/dyappd.toml`; compare it with the new default in
  `/etc/dyappd.toml.dpkg-dist` when `apt` offers one.

### Rollback

Roll the data back before the program. A newer `dyappd` upgrades older store files when it
starts, and an older one refuses files a newer one has upgraded:

| Store files | `dyappd` | Result |
|---|---|---|
| Older format | Newer | Upgraded in place on start; older versions can no longer open them |
| Same format | Any | Runs |
| Newer format | Older | Exits with "has store format N, this dyappd knows up to M" and changes nothing |

So never install the previous package alone over upgraded files. After a failed update:

1. `sudo systemctl stop dyappd`. The unit restarts a failing node every 5 seconds, and the
   restore cannot take `dyappd.lock` while a start is under way. A stopped unit stays stopped,
   and installing a package does not start it; do not retry the update until recovery is done.
2. Restore the [verified snapshot](#verified-pre-update-snapshot-planned) taken before the update
   with `restore-bootstrap.py`. Without one, restore the backup taken before the update
   ([First deployment](#first-deployment), step 4).
3. Install the previous package (`sudo apt install ./dyappd_<previous>.deb`) and
   `sudo systemctl start dyappd`, or start the previous image or tarball binary.

Writes are at risk only between the snapshot and the restore. The preflight holds
`dyappd.lock`, so nothing is written while it runs; whatever the new version wrote after it
started, and anything written if the node ran again after the snapshot, is lost. Other nodes keep
replicas of profiles, mailboxes and media, and owners repair what a node missed on their next
visit, but a single node has no other copy. The failed store stays in
`<storage.dir>.failed-<UTC time>` until you delete it.

### Deny list

To stop serving an abuser or a media blob, add their peer ID, IP group, key hash or the blob's
SHA-256:

```bash
sudo -u dyappd dyappd deny add <entry> [note]
sudo -u dyappd dyappd deny remove <entry>
sudo -u dyappd dyappd deny list
```

The running node applies a change within 10 seconds, or at once on `systemctl reload dyappd`;
denied peers get `STATUS_REFUSED` here and use other nodes. An IP group is given with the node's
prefix, e.g. `198.51.100.0/24`. Listing a blob hash removes its local whole copy, manifest
and shards on reload; keep and attachment records remain but report it missing. The node logs the
hash and increments `purged_blobs` in `dyappd status`. Removing the entry does not restore the
blob; its owner may upload it again. The list is local to the node and kept in `deny.db`. Deny-list
exchange between nodes (`share_deny_list`, `accept_deny_lists`) is not part of the first release:
a shared list tells other operators what you refuse, and received lists are only stored, to be
reviewed with `dyappd deny received`. Details: [deny list](../architecture/bootstrap.md#deny-list).

### Retiring a node

```bash
sudo apt purge dyappd
sudo rm -rf /var/lib/dyappd /etc/credstore.encrypted/node.key
sudo userdel dyappd
```

Tell the operators who list this node in `seeds` to remove it. Its replicas are repaired from the
other holders.

### Node key compromise

A copied `node.key` cannot be revoked, so the node starts over as a new node:

```bash
sudo systemctl stop dyappd
sudo rm -rf /var/lib/dyappd/*   # storage.dir: node.key, node.id and every store
sudo rm -f /etc/credstore.encrypted/node.key   # the sealed key, if used
sudo -u dyappd dyappd keygen - |   # prints the new peer ID
  sudo systemd-creds encrypt --with-key=auto --name=node.key - /etc/credstore.encrypted/node.key
sudo systemctl start dyappd
```

Keygen refuses while any data is left, so the wipe comes first. Consequences:

- The node has a new peer ID. The stolen key can still run a node under the old one, so tell the
  operators who list the old ID (a seed `host:port/<peer ID>`, a deny entry) to replace it or
  deny it with `dyappd deny add <old peer ID>`.
- What the old node held is lost to it; replication repairs profiles, mailboxes and media from
  the other holders, and a single node loses its only copy.
- Update your own seeds and anything else that pins the old ID.

### TURN relay

The `turn` role hands out credentials for a coturn relay that runs next to the node; `dyappd`
relays nothing itself. Calls are not part of the first release. Install coturn
(`apt-get install coturn` on Debian 12) and set in `/etc/turnserver.conf`:

```
use-auth-secret
static-auth-secret=<the same secret as turn.secret>
realm=turn.example.org
total-quota=100
user-quota=4
max-bps=1000000
no-multicast-peers
no-tcp-relay
denied-peer-ip=0.0.0.0-0.255.255.255
denied-peer-ip=10.0.0.0-10.255.255.255
denied-peer-ip=100.64.0.0-100.127.255.255
denied-peer-ip=127.0.0.0-127.255.255.255
denied-peer-ip=169.254.0.0-169.254.255.255
denied-peer-ip=172.16.0.0-172.31.255.255
denied-peer-ip=192.0.0.0-192.0.0.255
denied-peer-ip=192.168.0.0-192.168.255.255
denied-peer-ip=198.18.0.0-198.19.255.255
denied-peer-ip=240.0.0.0-255.255.255.255
denied-peer-ip=::1
denied-peer-ip=fc00::-fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff
denied-peer-ip=fe80::-febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff
denied-peer-ip=64:ff9b::-64:ff9b::ffff:ffff
no-cli
```

The `denied-peer-ip` ranges, `no-multicast-peers` and `no-tcp-relay` keep any peer holding
credentials from relaying into the operator's own network or to cloud metadata; coturn takes
single addresses or `a-b` ranges here, not CIDR. Relaying to public addresses stays possible,
bounded by the quotas.

Size the quotas from the relay's uplink. `max-bps` caps each session in bytes per second (1000000
is about 8 Mbit/s); `user-quota` caps the concurrent allocations of one username without its
expiry, which the node derives from the client's IP group, so clients behind one address share it;
`total-quota` caps all allocations, so set it to about the uplink divided by `max-bps`. The
example fits a 1 Gbit/s uplink with headroom.

Give the node the secret as a file through `turn.secret_file` (or `DYAPPD_TURN_SECRET` in a
systemd `EnvironmentFile=` with mode 0600). Open the
listening port (3478 UDP and TCP, 5349 for TLS) and the relay port range (`min-port`/`max-port`,
49152–65535 by default). At startup the node refuses the role without a secret or with a URL
that does not start with `turn:` or `turns:`. Credentials expire after `turn.credential_minutes`;
rotating the secret needs both services restarted together.

### Data cleanup and compaction

Expired envelopes, profiles, tombstones and attachments are deleted every hour, by the
[retention limits](#limits). No full `VACUUM` is needed: every `maintenance.interval_minutes` the
node returns up to `maintenance.vacuum_pages` free pages per store (`profiles.db`, `messages.db`
and the search index) to the file system, truncates the WAL and refreshes planner statistics. The
log lines "store maintenance done" and "search index maintenance done" show the free pages left; a steadily growing number means `vacuum_pages` is too small for the delete rate.
Background rebuilds share `maintenance.io_mb_per_s` and report "store rebuild progress".

## Backups

There is no online backup: SQLite files copied while the node writes are inconsistent. Stop the
node first.

| In `storage.dir` | Back up? |
|------------------|----------|
| `node.key` (unless sealed or set by `storage.key`) | **Yes**, privately: a lost key is a new node. A sealed key is backed up as the `age` copy made at keygen. |
| `node.id`, `network` | Yes: the node does not start on its stores without them. |
| `profiles.db`, `messages.db`, `media.db` and `data/` | Yes before an update (the only way back across a store-format change); otherwise optional, since replicas are repaired from other nodes. |
| `deny.db` | Yes: your deny list. |
| `profiles-idx.db` or `profiles-idx-vN.db`, `search.lock` | No: the search index is disposable and refilled by catch-up. |
| `peers`, `anchors` | No: the peer cache, rebuilt from the network. |
| `dyappd.lock` | No: the running node holds it so that a second node or an offline backup cannot use the directory at the same time. |

### Verified pre-update snapshot (planned)

The simple tar command below is a manual copy, not a verified recovery point.
The repository's [offline snapshot writer](../../tools/scripts/backup-bootstrap.py)
now produces the file inventory and manifest for rehearsal. Stop the node
before using it; it does not verify that the node is stopped or that the result
can be restored. The [independent verifier](../../tools/scripts/verify-bootstrap-backup.py)
checks the manifest, hashes and exact inventory, but does not probe SQLite or
the key; the automatic update gate is still planned. The full process must make a
stopped-node snapshot in a new, private directory with `storage/`, `recovery/`
and `manifest.jsonl`. From a repository checkout, after stopping the node, a
rehearsal with a plain inline key is:

```bash
sudo python3 tools/scripts/backup-bootstrap.py /var/lib/dyappd \
  /root/dyappd-backup-v1 --key-mode inline
sudo python3 tools/scripts/verify-bootstrap-backup.py /root/dyappd-backup-v1
sudo python3 tools/scripts/probe-bootstrap-sqlite.py /root/dyappd-backup-v1
sudo python3 tools/scripts/probe-bootstrap-backup.py /root/dyappd-backup-v1 \
  --dyappd /usr/bin/dyappd --id-pow-bits 22
```

For an external plain key or an `age`-encrypted key copy, choose `external`
or `age` and add `--recovery-key <path>`; the writer copies that file without
decrypting it. The destination must not exist and remains incomplete without
`manifest.jsonl` if copying fails. Do not use the resulting snapshot as an
unattended update gate until the separate updater enforces this preflight. The
SQLite probe first
verifies the manifest and inventory, refuses hot rollback journals, then copies
DB/WAL files to a private temporary directory and runs `integrity_check(1)`
there. It does not open the source snapshot with SQLite.
The full preflight then copies the saved markers and key into another private
temporary directory and runs `dyappd probe-key` with an empty config and the
original `network.id_pow_bits` value. For `external`, it uses the saved
`recovery/node.key`; for `age`, pass `--age-identity <private-key-file>` and
`--age-binary <age-executable>`. It decrypts the verified `recovery/node.key.age` copy into
scratch before probing, and fails if `age` or the identity is unavailable, the identity is
wrong or the ciphertext was altered. The tests run this against the official age 1.3.2
release. The preflight never opens or changes the source snapshot with SQLite.
Both probes use `TMPDIR` for these copies; `--scratch <dir>` picks another directory.

The [update preflight](../../tools/scripts/preflight-bootstrap-update.py) runs the whole
sequence in one command and is what the planned updater will call:

```bash
sudo python3 tools/scripts/preflight-bootstrap-update.py /var/lib/dyappd \
  /root/dyappd-backup-v2 --key-mode inline --dyappd /usr/bin/dyappd --id-pow-bits 22
```

It takes `dyappd.lock` and fails with "node still running" if a node holds it; while it
runs, the node cannot start. It then checks free space, writes the snapshot and runs both
probes, and exits non-zero on any failure without changing `storage.dir`. The space check
wants the snapshot size plus 64 MiB at the destination, the snapshot size again in the
scratch directory, and the largest store with its WAL free on the `storage.dir` file system,
since a store-format change may rebuild a table beside the old one; directories on one file
system add up. A node older than the data-directory lock never takes `dyappd.lock`, so the
preflight cannot tell that such a node is running: stop it first.

To go back to such a snapshot, for example after a new version upgraded the store and then
failed, stop the node and run the [restore](../../tools/scripts/restore-bootstrap.py) with the
same key options as the preflight:

```bash
sudo python3 tools/scripts/restore-bootstrap.py /root/dyappd-backup-v2 /var/lib/dyappd \
  --dyappd /usr/bin/dyappd --id-pow-bits 22
```

It takes `dyappd.lock`, runs both probes again and refuses a snapshot whose peer ID differs
from the `node.id` in `storage.dir`. It then copies `storage/` beside `storage.dir`,
checks every hash again, syncs it to disk with the directory's owner and mode, and swaps it in:
the old directory is kept as `<storage.dir>.failed-<UTC time>` for inspection and is never
deleted. Any failure before the swap leaves `storage.dir` unchanged. An external or sealed key
is not touched; only the `storage.dir` contents come back. Writes made after the snapshot are
lost.

The first JSON Lines record contains schema `1`, the saved peer ID and network,
the key mode (`inline`, `external` or `age`), and an explicit list of absent
optional stores. Following records, sorted by relative path, contain the byte
size and SHA-256 of every regular file. Version 1 accepts at most 1 million
files, a 256 MiB manifest, 4 KiB per record and 1 KiB per path, processed as
a stream; larger snapshots need a later format. Reject duplicate, absolute,
parent-traversal and unlisted paths, symlinks and other special files. Hashes
detect accidental damage; they do not authenticate a backup an attacker can
rewrite. Keep the whole snapshot encrypted or off the cloud provider.

`storage/` must hold `node.id`, `network`, `profiles.db` and `messages.db`.
Include `media.db`, `deny.db` and every regular file under `data/` when present;
record absence explicitly. Copy any existing `*.db-wal` or `*.db-journal` with
its database: after an interrupted shutdown they can hold recovery state.
`*.db-shm`, search indexes, locks and peer caches can be rebuilt. A hot
rollback journal blocks automatic update until recovery is checked in an
isolated copy. [SQLite's WAL guidance](https://www.sqlite.org/wal.html#the_wal_file)
and [journal guidance](https://www.sqlite.org/howtocorrupt.html#_deleting_a_hot_journal)
explain why the sidecars cannot be silently dropped.

For an inline key, include `storage/node.key`; for an external `storage.key`,
include a private `recovery/node.key` copy. A systemd-sealed credential is
bound to its host or TPM and is **not** portable recovery evidence: include an
independently encrypted `recovery/node.key.age` copy and require the operator
to provide its age identity to the restore probe, which decrypts only in scratch. Refuse an
unattended update if that copy or decryption proof is unavailable. Preserve
the node's configuration separately, especially its network name, ID proof of
work difficulty, roles and external key setting.

The preflight checks each SQLite database and asks `dyappd` to prove that the
recovered key matches `node.id`. It never opens or replaces live storage. The
unattended updater remains planned. Keep the pre-update snapshot
until the new version is accepted. A
restore discards writes made after the snapshot ([Rollback](#rollback)); do not
roll an upgraded SQLite store back in place.

A `traffic` file left by an older version is no longer read and can be deleted. Example with the
package layout:

```bash
sudo systemctl stop dyappd
sudo sh -c 'umask 077; tar czf /root/dyappd-$(date +%Y%m%d).tar.gz -C /var/lib dyappd'
sudo systemctl start dyappd
# Restore: stop, move /var/lib/dyappd aside, untar into /var/lib, start, check status
```

Keep backups that hold `node.key` off the cloud provider, or encrypted with a key it does not
have. Scheduled backups, retention and off-host storage are not provided.

## Security

### Network

Every connection is libp2p: TCP with Noise and yamux, or QUIC (TLS 1.3), authenticated by the
node's key; there is no certificate authority. The node protocol is public: anyone can read any
profile, and only the owner's signature can store, replace or delete one. Requests are limited
per peer, per IP group and per sender key, and peers that keep hitting the limits or sending bad
signatures are banned locally for `ban_minutes`.

### Clock

Retention, expiry and the 10-minute windows for signed times use the node's clock. A clock pushed
forward, for example by a spoofed NTP server, makes the hourly cleanup delete envelopes, profiles
and attachments early; one pushed back refuses fresh heartbeats and keeps. Synchronise the host
with an authenticated source and watch for large steps in the `chrony` log.

Plain NTP answers are not signed: anyone on the path between the host and its time server can
forge them and move the clock. Network Time Security (NTS, RFC 8915) adds a TLS handshake with the
server (NTS-KE, TCP port 4460), which checks the server's certificate and hands out keys; every
later NTP reply (UDP port 123) carries an authentication tag made with those keys. `chrony` then
uses only replies that really come from that server and were not changed on the way.

NTS does not make the time correct by itself. A server you chose can still send wrong time, the
VM host and anyone with root on it can still set the clock, and a plain NTP source next to the NTS
ones can still pull the clock unless unauthenticated sources are refused. Prefer two or three NTS
servers from different operators. Debian 12 ships `chrony` 4.3, which supports NTS:

```bash
sudo apt-get install -y chrony
# Comment out the default `pool` line in /etc/chrony/chrony.conf, then:
sudo tee /etc/chrony/sources.d/nts.sources >/dev/null <<'EOF'
server time.cloudflare.com iburst nts
server nts.netnod.se iburst nts
server ptbtime1.ptb.de iburst nts
EOF
echo 'authselectmode require' | sudo tee /etc/chrony/conf.d/nts.conf
sudo systemctl restart chrony
chronyc -N authdata   # each server: Mode NTS, a non-zero KeyID and Cook column
chronyc sources       # `^*` marks the server in use
chronyc tracking      # "Leap status: Normal" and a small "System time" offset
```

`authselectmode require` makes `chrony` ignore every source without authentication. The firewall
must allow outgoing TCP 4460 and UDP 123. If `authdata` shows `-` for the mode or zero cookies,
the NTS handshake failed: check that port 4460 is open and the server name is right.

### Cloud hosting

On a rented VM, assume that the provider, or anyone holding your cloud account, can read the
disk, its snapshots and backups, write to the disk and read the VM's memory. They can then copy
the node key and the TURN secret, see what you see as an operator, and replace `dyappd` with a
backdoored build; they cannot forge users' signed records or read end-to-end content
([threat model](../security/threat-model.md#cloud-host-access)). A sealed key bound to a vTPM
keeps a copied volume from yielding the node key; only a confidential VM (AMD SEV-SNP, Intel TDX)
keeps the memory of a running VM from the host. There is no encryption of the stores at rest and
no start-time integrity check, so:

- Protect the cloud account with hardware-key 2FA and limit who may snapshot or attach volumes.
- Keep backups that hold the key off the provider, or encrypted with a key it does not have.
- If you suspect access, reinstall from a package you build or verify yourself, follow
  [Node key compromise](#node-key-compromise) and rotate the [TURN](#turn-relay) secret.

### Best practices

- Pin every seed with its peer ID (`host:port/<peer ID>`).
- Keep the production defaults of `[network]` on the public network.
- Run several nodes in different /16 networks and providers; restart them one at a time.
- Keep the metrics listener on loopback and scrape it locally.
- Watch free disk space, `dyapp_storage_read_ok` and the snapshot age.
- Take a stopped backup before every update.

## Capacity

Not measured: there is no load test yet. Disk, the byte rate, memory and connections are capped
by `[limits]`. With the default quotas a node needs about 16 GiB of disk (`profiles_max_mb`
1 GiB + `messages_max_mb` 4 GiB + `media_max_mb` 10 GiB + `min_free_mb` 512 MiB; without the
media role about 6 GiB; the default search role adds up to `search.max_bytes`, 50 GiB, as the
network grows, so lower it on a small disk) and 1 GiB of memory
(`MemoryMax=1G`, `limits.max_memory_mb` 768). A full store answers `FULL` instead of growing.

## Troubleshooting

| Symptom | Check |
|---------|-------|
| `dyappd info` fails with a timeout or `request failed` | The node is down, the address is wrong (`host:port`, with an IPv6 address in brackets) or a firewall blocks TCP/UDP 33445 |
| `dyappd info` works on the host but times out from elsewhere | `listen` is the loopback default; set `listen = ["[::]:33445", "0.0.0.0:33445"]` |
| Writes answer `STATUS_FULL` | The node trusts no peer yet (`storage_trust_minutes` after its start), a store is at its quota, or the disk is below `min_free_mb` |
| `dyappd` exits at startup about a directory | The storage directory is not writable by the node's user; fix permissions or `ReadWritePaths=` |
| `run as the owner of ...` | Run commands as the service user: `sudo -u dyappd dyappd ...` |
| `dyappd` exits with `storage belongs to ...` | `storage.dir` was bound to another `network.name`; restore that name or use a new directory |
| `dyappd info` fails against a named-network node | The client speaks its config's `network.name`; run it with the node's config or add `--network.name <name>` |
| `dyappd` exits with `has store format N, this dyappd knows up to M` | A newer `dyappd` upgraded that file; run that version or restore the backup taken before it ([Updates](#updates)) |
| `dyappd` exits with `has search index format N, this dyappd knows up to M` | A newer `dyappd` rebuilt the search index; run that version, or delete the named `profiles-idx-vN.db`: catch-up refills it |
| `dyappd` exits with `search.lock: ...; another dyappd uses this search index` | Two nodes share one `storage.dir`; give each its own directory |
| `dyappd` exits with `dyappd.lock: ...; another dyappd or a backup uses this storage.dir` | Another node runs on this `storage.dir`, or a backup is still copying it; stop the other node or wait for the backup |
| `not listening` warning or `no address could be opened` | The port is taken (another `dyappd`?) or IPv6 is disabled on the host; adjust `listen` |
| Key refused with a `chmod 600` hint | The key file has group or other permission bits |
| Disk keeps growing | Profiles stay for `limits.profile_ttl_days` after their owner's last request; lower it or the store quotas |
