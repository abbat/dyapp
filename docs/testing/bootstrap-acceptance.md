# Bootstrap node acceptance tests

> **Status: draft, awaiting sign-off.** The bootstrap node is accepted when every row below
> marked **exists** passes in CI and every **gap** is either closed or explicitly accepted by the
> reviewer. Behaviour itself is described in [bootstrap.md](../architecture/bootstrap.md).

## Layers

| Layer | What runs | Where | Command | In CI |
|-------|-----------|-------|---------|-------|
| **Unit** | One module, in process, no sockets | `#[cfg(test)]` in `rust/bootstrap/src/*.rs`, `rust/p2p-net/src/lib.rs` | `make test` | ci-base `core` |
| **Contract** | Golden wire/config fixtures and properties | `rust/bootstrap/tests/compatibility.rs`, `tests/properties.rs` | `make test` | ci-base `core` |
| **Functional** | One real `node::run` on loopback TCP, libp2p clients in the test | `rust/bootstrap/tests/protocol.rs` | `make test` | ci-base `core` |
| **E2E local** | Three `dyappd` processes on loopback, driven by `test-peer` | `scripts/network-test.py --local` | `make test` (dev image) | ci-base `network` |
| **E2E Docker** | Three `dyappd` containers and a coturn relay on a Docker network | `docker/compose.network.yml`, `scripts/network-test.py` | `make test-integration` | no (local only) |

## Coverage map

Status: **exists** = a test proves it today; **partial** = proved at a lower layer only;
**gap** = not proved.

### Wire protocol and compatibility

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Every request type answers (`info`, publish/get, mailbox put/challenge/fetch/ack) | Functional | `protocol.rs::every_request_type` | exists |
| Same bytes over TCP and QUIC (publish TCP, get QUIC) | E2E | `network-test.py::suite` | exists |
| Undecodable or oversized request closes only its stream; the node answers the next client | Functional | `protocol.rs::bad_input_fails_only_its_request` | exists |
| Unknown or empty request variant → `UNSUPPORTED` | Functional | `protocol.rs::bad_input_fails_only_its_request` | exists |
| Role not served (media, TURN) → `UNSUPPORTED` | E2E | `network-test.py::media`, `::turn` | exists |
| v1 message, signed profile, config and deny diagnostics stay byte-compatible | Contract | `compatibility.rs` | exists |
| **Mixed versions:** an older released `dyappd` answers a newer request with `UNSUPPORTED` and keeps serving | E2E | — | **gap** |

### Profiles and mailboxes

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Version order, `STALE` with the stored record, forged payload, tombstone | Unit, E2E | `storage.rs::signed_profile_versions_and_tombstone`, `service.rs::publish_get_stale_and_errors`, `network-test.py::suite` | exists |
| Profile expires after the owner was last seen; heartbeat keeps it | Unit | `storage.rs::profiles_expire_after_the_owner_was_last_seen`, `service.rs::heartbeat_keeps_the_profile_seen` | exists |
| Mailbox once per id, limits, fetch with challenge, ack, replay and forgery refused | Unit, E2E | `storage.rs::envelopes_once_per_id_limits_and_expiry`, `service.rs::mailbox_put_fetch_ack_forged_and_replayed`, `network-test.py::mailbox` | exists |
| Challenge is bound to its connection | Functional | `protocol.rs::challenge_is_bound_to_its_connection` | exists |
| Watching fetch gets a push | E2E | `network-test.py::mailbox` | exists |

### Replication, placement and repair

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| DHT routing finds every node through a seed | E2E | `network-test.py::network` | exists |
| Profile and mailbox replicas land on the DHT holders; ack forwarding | E2E | `network-test.py::replicate`, `::acked_everywhere` | exists |
| Without a second holder, profile and mailbox puts answer `FULL` | E2E | `network-test.py::local` | exists |
| A holder offline during a newer publish is repaired by heartbeat | E2E | `network-test.py::profile_repair` | exists |
| Churn: replicas skip a stopped node | E2E | `network-test.py::network` (churn) | exists |
| Repair fills gaps once an hour within budget | Unit | `node.rs::profile_repair_only_fills_gaps_once_an_hour_within_budget` | partial |

### Media

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Keep, put, get; whole copy and Reed–Solomon shards on all holders | Unit, E2E | `service.rs::media_*`, `network-test.py::media_spread` | exists |
| 3 MiB sharded blob readable after losing a node with ≤ 4 shards | E2E | `network-test.py::network` (churn) | exists |
| Deleted copy or shard restored by repair | E2E | `network-test.py::media_repair` | exists |
| Expiry, keep refresh and startup cleanup | Unit, E2E | `media.rs::*expire*`, `network-test.py::media_retention` | exists |
| Third stream per connection / seventeenth overall rejected before reading | Unit | `p2p-net` `media_permits_*`, `third_media_stream_*` | exists |

### TURN and relays

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| TURN node announces itself as a relay provider in the DHT | E2E | `network-test.py::network` | exists |
| Credentials follow coturn's `use-auth-secret` scheme | E2E | `network-test.py::turn` | exists |
| coturn allocates with the node's credentials and refuses a wrong password | E2E Docker | `network-test.py::turn` (`relay=True`) | **gap in CI** (local only) |

### Abuse limits and resources

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Per-peer rate limit: burst passes, rest `RATE_LIMITED`, nothing dropped | Unit, Functional, E2E | `rate_limit.rs`, `protocol.rs::requests_over_the_limit_are_refused`, `network-test.py::network` (flood) | exists |
| Strikes ban until quiet; group and sender quotas | Unit | `rate_limit.rs::strikes_ban_until_quiet`, `service.rs::group_and_sender_quotas_and_bans` | exists |
| Connections over the count limit are refused | Functional | `protocol.rs::connections_over_the_limit_are_refused` | exists |
| **Memory threshold:** connections over the memory share are refused | Functional | `protocol.rs::connections_over_the_memory_limit_are_refused` | partial: no multi-node load |
| Full stores and byte rate refuse requests | Unit | `service.rs::full_stores_and_byte_rate_refuse_requests` | exists |

### Deny lists

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Operator deny list refuses a peer or IP group without a strike | Unit | `service.rs::deny_list_refuses_without_strikes` | partial |
| Received signed lists are verified and only newer ones kept | Unit | `deny.rs::received_lists_are_checked_and_only_newer_ones_kept` | partial |
| **Deny over the network:** a running node refuses a denied client, and a shared list reaches another node | E2E | — | **gap** |

### Storage and configuration

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Data survives reopen and restart | Unit, E2E | `storage.rs::data_survives_reopen`, `network-test.py::media_retention` | exists |
| Store format steps run once; a newer file is refused | Unit | `storage.rs::store_format_steps_run_once_and_a_newer_file_is_refused` | exists |
| Maintenance frees pages in bounded steps | Unit | `storage.rs::maintenance_frees_pages_in_bounded_steps`, `maintenance.rs` | exists |
| Config precedence file < env < CLI; unknown keys fail; node key bound to the data | Unit, Property | `config.rs` | exists |

## Gaps

1. **Mixed versions.** No test runs an older `dyappd` against a newer client. It needs either a
   released binary in the test image or a test mode that answers as an older version.
2. **Deny lists end to end.** A denied `test-peer` key or IP group against a running node, and
   propagation of a signed shared list (`network.share_deny_list`) to a second node.
3. **coturn allocation in CI.** Only `make test-integration` runs the Docker network with a
   real coturn; CI runs the loopback suite. Either add the Docker network to CI or accept it as
   a local-only check.
4. **Memory threshold under load.** Proved on one node with `max_memory_mb = 1`; not with
   several nodes replicating at the same time.
5. **Time-driven behaviour** (profile expiry, hourly repair, keep expiry) is tested in units and
   by ageing fixture rows; a deterministic clock is planned through a gated test-mode runtime.
6. **Functional layer clients.** `protocol.rs` uses in-process libp2p clients; the `test-peer`
   binary is exercised only by the network suite. Decide whether that is enough.

## Sign-off

Pending: the reviewer records acceptance (or the gaps to close first) as a review comment.
