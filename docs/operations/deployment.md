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
  ├─ REST API (message relay)
  ├─ Profile index (search by age/location)
  ├─ Peer discovery (announce presence)
  └─ Replication (planned; whole records to 5 points, K=6/M=4 for large media, see ADR 0009)

                    ↓ (plaintext today; encryption planned)

Users (iOS/Android/macOS/Linux)
  └─ Mailbox on the recipient's nodes (planned; pushed at once when the recipient is online)
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

> **Status:** there is no production bootstrap executable yet.
> `rust/bootstrap` is a library: it exports `BootstrapServer::router(AppState)`
> and `BootstrapStore`, but has no `src/main.rs`, no CLI parser and no
> `dyapp-bootstrap` binary. Flags such as `--listen-port` or
> `--storage-path` do not exist. The only runnable server is the development
> binary `test-peer`, described below. A real server binary with configuration
> needs a separate implementation task.

### What the host program has to do

Any executable that serves the API builds the state and router itself, as
`rust/bootstrap/src/bin/test-peer.rs` does:

```rust
let config = BootstrapConfig { storage_path: "/path/to/data".into(), ..BootstrapConfig::default() };
let state = AppState {
    store: Arc::new(BootstrapStore::new(&config.storage_path)?),
    rate_limiter: Arc::new(PeerRateLimiter::new(100)),
    config,
};
let listener = tokio::net::TcpListener::bind("0.0.0.0:7070").await?;
axum::serve(listener, BootstrapServer::router(state)).await?;
```

The library does **not** handle: TLS, authentication, logging, metrics,
signal handling, TTL cleanup scheduling, or reading `listen_addr` /
`listen_port` from `BootstrapConfig` (the host binds the socket). These are
the host's responsibility, and none of them is implemented today.

### Development quick start (`test-peer`)

`test-peer` hard-codes its settings: it binds `0.0.0.0:7070` and stores
RocksDB data in `/tmp/ai/bootstrap`. It ignores `listen_addr` / `listen_port`
and takes no arguments. Use it for local development only.

**In Docker (no host Rust toolchain needed).** The network-test image builds
`test-peer` (`docker/Dockerfile.network-test`):

```bash
make prepare            # builds the dev, UI-test and network-test images
make test-integration   # integration tests + two test-peer containers on an internal network

# Manual health check in a throwaway, network-less container:
docker run --rm --network none --user 999:999 --tmpfs /tmp:rw,exec,mode=1777 \
  dyapp:network-test bash -c '
    target/debug/test-peer &
    sleep 3
    python3 -c "import urllib.request; print(urllib.request.urlopen(\"http://127.0.0.1:7070/health\").read().decode())"
    kill %1'
# → {"status":"healthy","timestamp":<unix seconds>}
```

**With a local Rust 1.99 toolchain:**

```bash
cargo run -p dyapp-bootstrap --bin test-peer
curl http://localhost:7070/health
# → {"status":"healthy","timestamp":<unix seconds>}
```

Prerequisites: Rust 1.99 (`rust-toolchain.toml`) and a C/C++ toolchain for
RocksDB, or Docker for the containerised path.

### Multi-node cluster (planned)

Running several `test-peer` processes gives independent servers: there is no
replication or data exchange between them. HA and Reed–Solomon replication
are design targets; see [Bootstrap Servers](../architecture/bootstrap.md).

### Docker and Kubernetes (planned)

There is no production Dockerfile, published image or Kubernetes manifest.
The repository's Dockerfiles (`docker/Dockerfile.dev`, `docker/Dockerfile.network-test`, …)
are test images: they build in debug mode and run `test-peer`. A production
image and manifests need a server binary first.

## Client Configuration (planned)

No platform app connects to a bootstrap server yet, and there is no client API
for configuring bootstrap URLs (`rust/p2p-net/src/discovery.rs` only keeps a
list of addresses; `query_bootstrap` is a stub). The intended behaviour is a
built-in list of bootstrap URLs plus a user-editable setting.

## Monitoring

### Health check (exists)

`GET /health` is the only operational endpoint (`rust/bootstrap/src/api.rs`).
It runs `BootstrapStore::health_check` (writes and deletes a temporary `health:<uuid>` key in RocksDB) and returns:

```bash
curl http://localhost:7070/health
# 200 → {"status":"healthy","timestamp":<unix seconds>}
# 503 → {"status":"unhealthy","timestamp":<unix seconds>}
```

There are no counts, peer numbers or replication fields in the response.

### Not implemented

These do not exist and need a separate implementation task before they can be
documented as runnable:

- `/metrics` (Prometheus) and any metric names
- `/admin/cleanup`, `/admin/replication-status` or any other admin route
- Logging: `tracing` is a dependency, but no subscriber is initialised and the
  server emits no log lines, so `RUST_LOG` has no effect

## Maintenance

### Data cleanup

**TTL cleanup is not running.** Records get `ttl_expires_at` (messages 24 h,
profiles 30 days), but nothing in the repository calls
`BootstrapStore::cleanup_expired`: no scheduler, no endpoint, no test. Reads
do not filter expired records either, so data is kept until a client sends
`DELETE`. See [Privacy](../security/privacy.md#retention).

### Backups

There is no online backup: copying a RocksDB directory while the server writes
to it does not give a consistent snapshot. Stop the process first.

Verified roundtrip with `test-peer` (data in `/tmp/ai/bootstrap`):

```bash
# 1. Stop writers
kill <test-peer pid>

# 2. Archive with a relative layout (top-level entry: bootstrap/)
tar czf bootstrap-backup-$(date +%Y%m%d).tar.gz -C /tmp/ai bootstrap

# 3. Restore into an empty test directory and inspect it
mkdir /tmp/restore-test
tar xzf bootstrap-backup-YYYYMMDD.tar.gz -C /tmp/restore-test   # → /tmp/restore-test/bootstrap

# 4. Swap it in, keeping the old copy for rollback, then restart and check
mv /tmp/ai/bootstrap /tmp/ai/bootstrap.old
mv /tmp/restore-test/bootstrap /tmp/ai/bootstrap
cargo run -p dyapp-bootstrap --bin test-peer &
curl http://localhost:7070/messages/<known message id>   # must return the record

# Rollback: stop the server, move bootstrap.old back
```

Scheduled backups, retention and off-host storage are not provided.

### Updates

**Rolling update (planned).** Without cross-node replication, taking a node
offline makes its stored messages and profiles unavailable until it returns;
other nodes do not hold copies. Rolling upgrades need the replication protocol
and a node-failure test first.

## Security

### Network (target; not implemented: the server speaks plain HTTP)

- **Bootstrap → Client:** TLS 1.3 (Let's Encrypt)
- **Bootstrap → Bootstrap:** mTLS (certificate pinning)
- **Client → Peer:** QUIC/TLS 1.3 (direct or via TURN relay)

### Access Control

**Bootstrap API is public** (no authentication):
- Any client can store, read, overwrite or delete any message or profile
- Only the two POST endpoints are rate-limited, per client-supplied ID; the
  rate is chosen by the host program (`test-peer`: 100/s) and rotating IDs
  bypasses it. Put a reverse proxy with per-IP limits in front of any exposed
  instance. Details: [REST API](../architecture/bootstrap.md#rest-api)

**Admin API (future):**
- Cleanup, monitoring, replication status
- Secured with API key (env var)

## Cost and capacity

Not measured. There is no load test of the bootstrap server, and `max_peers`
in `BootstrapConfig` is not enforced. Multi-node "HA" setups are not possible
yet (see [replication](../architecture/bootstrap.md#replication-strategy-reed-solomon)).

## Troubleshooting

Only what applies to the code that exists today:

| Symptom | Check |
|---------|-------|
| `/health` returns 503 | RocksDB probe failed: check that the storage directory is writable and the disk is not full |
| `test-peer` exits with `StorageError(... Permission denied)` | The process cannot create `/tmp/ai/bootstrap`; fix permissions or run with a writable `/tmp` |
| Port 7070 already in use | Another `test-peer` is running; `test-peer` cannot change its port |
| Disk keeps growing | Expected: TTL cleanup does not run (see [Data cleanup](#data-cleanup)) |

There is no systemd unit, start/stop script, service name or log to inspect;
these are future artifacts that ship together with a server binary.

## Next Steps

Work that turns this guide into a production runbook is tracked in GitHub
Issues: a server binary with configuration, TLS, authentication,
logging/metrics, a TTL cleanup scheduler, and cross-node replication.

## References

- [Bootstrap Server Architecture](../architecture/bootstrap.md)
- [Testing & CI/CD](../testing/README.md)
