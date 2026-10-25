# Bootstrap node acceptance tests

> **Status: signed off.** The bootstrap node is accepted when every row below marked **exists**
> passes in CI (or in its named local command) and every remaining gap is closed or accepted under
> [Decisions](#decisions). Behaviour itself is described in
> [bootstrap.md](../architecture/bootstrap.md). Every check runs in at most 5–10 minutes; there are
> no nightly or long-running suites.

## Layers

Configuration seed parsing and real pinned-key negotiation tests live in
`src/config/seeds.rs`, preserving their original assertions and timeouts.
Search/incoming-group/general configuration boundaries are covered in
`src/config/validation.rs`, including literal network-name validation; shared temporary
fixtures remain test-only. Server clock-hint tests of node info, heartbeats and media live in
`src/service/clock.rs`.

Search filter/place/scan-budget and invalid-condition unit tests live in
`src/search/query.rs`, using the existing SQLite fixtures in `search.rs`.
Record lifecycle, heartbeats, expiry and eviction tests live in
`src/search/lifecycle.rs`. Shared helpers are visible only to sibling test
modules. Digest mutation and inventory boundary cases live in `src/search/digest.rs`;
the existing digest-rebuild fixture remains in `search.rs`. Every search source
file now meets the 1000-line limit without an exception. These extractions
preserve runtime behavior, schema and assertions.

| Layer | What runs | Where | Command | In CI |
|-------|-----------|-------|---------|-------|
| **Unit** | Module tests in process; networking tests also use real loopback sockets | `#[cfg(test)]` in `rust/bootstrap/src/*.rs`, `rust/p2p-net/src/lib.rs` | `make test` | ci-base `core` |
| **Contract** | Golden wire/config fixtures and properties | `rust/bootstrap/tests/compatibility.rs`, `tests/properties.rs` | `make test` | ci-base `core` |
| **Functional** | One real `node::run` on loopback TCP, libp2p clients in the test | `rust/bootstrap/tests/protocol.rs` | `make test` | ci-base `core` |
| **E2E local** | Three `dyappd` processes on loopback, driven by `dyappd`'s client commands | `tests/scripts/network-test.py --local` | `rust-check.sh network` (no Make target) | ci-base `network` |
| **E2E Docker** | Three `dyappd` containers and a coturn relay on a Docker network | `tests/docker/compose.network.yml`, `tests/scripts/network-test.py` | `make test-integration` | no (local only) |
| **Memory plateau** | The suite's mailbox and media load repeated on three release `dyappd` for 5 minutes | `tests/scripts/network-test.py --plateau` | `make test-integration` (`rust-check.sh memory`) | no (local only) |
| **Rehearsal** | Cold start, two-holder canary, rolling upgrade from an older commit and rollback on three release `dyappd` with production defaults | `tests/scripts/rehearsal.py` | `make rehearsal FROM=<git ref>` (`rust-check.sh rehearsal`) | manual `rehearsal.yml`, about 3 h |
| **Package** | The `.deb` and the tarball installed on Debian 12, node started as its user | `tests/scripts/deb-test.sh`, `tests/scripts/tarball-test.sh` | `make deb` | no (local only) |

The functional suite keeps shared node/client fixtures in `protocol.rs`.
Its child modules under `tests/protocol/` group namespace isolation and service
negotiation (`namespaces.rs`), signed-time errors and boundaries (`clock.rs`),
exact TCP/gossip wire accounting (`wire.rs`), and search spreading between nodes and worker
admission (`search.rs`).
The split preserves every existing test body and actual loopback assertion;
all files now satisfy the 1000-line size limit without an exception.

The local E2E and memory plateau suites allow 1000 incoming connections per IP group: their
short-lived clients all share loopback and can reach the default cap before idle cleanup. The
default 32 budget is covered separately by real TCP tests in `node/tests/groups.rs`, including pending handshakes,
established connections, close/failure slot reuse, outgoing exemption and composed refusals.
A 100-connection QUIC churn test checks guard reservations and groups return to zero.

Network and memory stages print a concise result through `rust-check.sh`.
The standalone `network-test.py` CLI also uses that contract, including health
checks; `--raw` selects its direct diagnostics. Rust stages select raw mode
internally to retain one owning wrapper. Invalid CLI arguments fail before DNS
or workload setup; `--help` prints the command syntax without running a suite.
Completed plateau observations retain timestamped per-round RSS for every node,
all first/second-half peaks and failed nodes, the unchanged limits and duration,
and the measured binary SHA-256 in `build/memory-plateau-<unique-id>.json`.
Set `DYAPP_MEMORY_REPORT_DIR` to another writable directory if needed. The report
is saved before the RSS gate raises; too few rounds or an interrupted workload
still fail and do not produce a completed observation. This evidence helps
separate warmup from retained growth; a passing rerun alone proves neither a
root cause nor a fix. The [RSS investigation](memory-plateau-investigation.md)
records an observed gate failure and the remaining attribution questions.
On failure, open the retained raw log named by `DETAILS` to identify the failed
protocol assertion or RSS sample, then use the corresponding coverage-map row
to find its implementation. Reproduce with the command in the Layers table;
`bash tests/scripts/rust-check.sh --raw network` (or `memory`) keeps direct diagnostic
output. Fix the underlying assertion or growth before rerunning; do not relax
connection limits or the plateau tolerance to make a regression pass.

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
| Role not served (media, TURN) → `UNSUPPORTED` | E2E | `network_checks.py::media`, `::turn` | exists |
| v1 message, signed profile, config and deny diagnostics stay byte-compatible | Contract | `compatibility.rs` | exists |
| **Mixed versions:** an older released `dyappd` answers a newer request with `UNSUPPORTED` and keeps serving | E2E | — | deferred: [Decisions](#decisions) |

### Profiles and mailboxes

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Version order, `STALE` with the stored record, forged payload, tombstone | Unit, E2E | `storage.rs::signed_profile_versions_and_tombstone`, `service/tests.rs::publish_get_stale_and_errors`, `network-test.py::suite` | exists |
| Profile expires after the owner was last seen; heartbeat keeps it | Unit | `storage.rs::profiles_expire_after_the_owner_was_last_seen`, `service/tests.rs::heartbeat_keeps_the_profile_seen` | exists |
| Mailbox once per id, limits, fetch with challenge, ack, replay and forgery refused | Unit, E2E | `storage.rs::envelopes_once_per_id_limits_and_expiry`, `service/tests/mailbox.rs::mailbox_put_fetch_ack_forged_and_replayed`, `network_checks.py::mailbox` | exists |
| Challenge is bound to its connection | Functional | `protocol.rs::challenge_is_bound_to_its_connection` | exists |
| Node info and signed-time failures carry clock hints; valid future boundary keeps prior statuses; non-clock errors remain unhinted | Functional, contract, unit | `protocol/clock.rs::signed_time_failures_keep_clock_hints_over_real_tcp`, `::future_clock_boundary_preserves_valid_request_statuses_over_tcp`, `compatibility.rs::server_clock_hints_keep_fixed_tags_and_legacy_decoding`, `service/clock.rs` | exists |
| Watching fetch gets a push | E2E | `network_checks.py::mailbox` | exists |
| Two clients with SQLite stores chat across restarts: an unsent Welcome is retried with its exact bytes, a storage fault at send puts nothing, one at receive shows and acks nothing, and a retried envelope is shown once | Functional | `protocol/chat.rs::durable_chat_survives_restarts_storage_faults_and_retries` | exists |
| Two sessions deliver text, then stage edit and terminal delete offline; after each restart `retry_due` sends the original encrypted write and operation ID, both projections converge, the receiver ACKs all envelopes, and its inbox retains only body digests; wrong-group/author local sends and duplicate delivery cannot change the projection | Functional | `protocol/chat_edits.rs::two_sessions_converge_on_edit_and_terminal_delete` | exists |
| Authenticated edit/delete projection isolates group and author, resolves early operations, rejects conflicting replay, and keeps deletion terminal across restart | Unit | `store/messages/operations/tests.rs::ordered_operations_replay_and_terminal_delete_survive_restart`, `::early_edits_resolve_once_and_discard_pending_text`, `::early_delete_is_terminal_and_pending_budget_is_global` | exists |
| Recipient ACK removes an original from its cooperating mailbox node; re-putting that original after a terminal delete is invisible and re-ACKed | Functional | `protocol/chat_edits.rs::two_sessions_converge_on_edit_and_terminal_delete` | exists |

The clock checks cover node replies. Client signing-time correction using a median from
three distinct nodes remains planned.

### Replication, placement and repair

Media-repair scheduling cursor/window/rolling-hour assertions live in
`src/media_repair/schedule.rs`; transfer and recoverability fixtures live
in `src/media_repair/tests.rs`. Both files meet the source-size limit, with no runtime or
assertion changes.

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| DHT routing finds every node through a seed | E2E | `network-test.py::network` | exists |
| Profile and mailbox replicas land on the DHT holders; ack forwarding | E2E | `network_checks.py::replicate`, `::acked_everywhere` | exists |
| Without a second holder, profile and mailbox puts answer `FULL` | E2E | `network-test.py::local` | exists |
| A holder offline during a newer publish is repaired by heartbeat | E2E | `network-test.py::profile_repair` | exists |
| Churn: replicas skip a stopped node | E2E | `network-test.py::network` (churn) | exists |
| Repair fills gaps once an hour within budget | Unit | `node/tests.rs::profile_repair_only_fills_gaps_once_an_hour_within_budget` | partial |

### Media

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Keep, put, get; whole copy and Reed–Solomon shards on all holders | Unit, E2E | `service/tests/media.rs::media_*`, `network_checks.py::media_spread` | exists |
| 3 MiB sharded blob readable after losing a node with ≤ 4 shards | E2E | `network-test.py::network` (churn) | exists |
| Deleted copy or shard restored by repair | E2E | `network-test.py::media_repair` | exists |
| Expiry, keep refresh and startup cleanup | Unit, E2E | `media.rs::*expire*`, `network-test.py::media_retention` | exists |
| Third stream per connection / seventeenth overall rejected before reading | Unit | `p2p-net` `media_permits_*`, `src/lib/media_streams.rs` | exists |

### TURN and relays

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| TURN node announces itself as a relay provider in the DHT | E2E | `network-test.py::network` | exists |
| Credentials follow coturn's `use-auth-secret` scheme | E2E | `network_checks.py::turn` | exists |
| coturn allocates with the node's credentials and refuses a wrong password | E2E Docker | `network_checks.py::turn` (`relay=True`) | exists, local only: [Decisions](#decisions) |

### Search

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Publish, query and get; a claim below `search.min_profile_pow_bits` is `INVALID` with the required bits, a forged one `DENIED` | Unit | `service/tests/search.rs::search_publish_query_get`, `::search_profile_pow`, `::search_pow_hint_is_specific_to_insufficient_difficulty` | exists |
| A search heartbeat needs a daily proof: missing, weak or another day `INVALID`, forged `DENIED`, cached once confirmed, limits checked before Argon2id | Unit | `service/tests/search.rs::search_heartbeat_pow`, `::heartbeat_just_before_midnight_passes`, `pow.rs::caches_confirmed_proofs_only` | exists |
| A published profile reaches another search node over its gossip topic; a named network serves search on its own topics | Functional | `protocol/search.rs::search_profile_spreads_between_nodes`, `protocol/namespaces.rs::named_node_serves_profile_search_and_configured_gossip_topics` | exists |
| A `search.topics` subset is refused on the public network | Unit | `config/validation.rs` | exists |
| Catch-up asks two distinct mesh peers per topic and skips the update limit | Unit | `node/tests.rs::catch_up_asks_two_distinct_peers_per_topic`, `search/lifecycle.rs::catch_up_skips_the_update_limit`, `service/tests/search.rs::search_peers_sync_on_their_own_budget` | exists |
| Query conditions, place `in` and the scan budget (`partial`); busy workers answer `RATE_LIMITED` without stalling the node | Unit, Functional | `search/query.rs::query_filters_place_in_and_budget`, `protocol/search.rs` | exists |
| The derived digest is rebuilt on open; expiry and inventory pages | Unit | `search.rs::digest_rebuilt_on_open`, `search/lifecycle.rs::expiry_and_inventory`, `search/digest.rs` | exists |
| A lost `profiles-idx.db` is refilled by catch-up between real nodes | E2E | — | missing |

### Abuse limits and resources

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Per-peer rate limit: burst passes, rest `RATE_LIMITED`, nothing dropped | Unit, Functional, E2E | `rate_limit.rs`, `protocol.rs::requests_over_the_limit_are_refused`, `network-test.py::network` (flood) | exists |
| Strikes ban until quiet; group and sender quotas | Unit | `rate_limit.rs::strikes_ban_until_quiet`, `service/tests/mailbox.rs::group_and_sender_quotas_and_bans` | exists |
| Connections over the count limit are refused | Functional | `protocol.rs::connections_over_the_limit_are_refused` | exists |
| Fine IP group shares 32 pending/established incoming slots; outgoing exempt; failure/close/composed refusal frees slots; QUIC churn drains | Real TCP/QUIC, unit | `node/tests/groups.rs::incoming_group_caps_pending_tcp_and_releases_failed_handshakes`, `::incoming_group_caps_established_tcp_but_exempts_outgoing_and_releases_close`, `::composed_peer_limit_refusal_does_not_leak_incoming_group_reservation`, `::quic_churn_releases_incoming_groups`, `p2p-net/src/incoming_groups.rs` | exists |
| Connections over the memory share are refused | Functional | `protocol.rs::connections_over_the_memory_limit_are_refused` | exists |
| Idle nodes drain: nothing pending, no watch, no entry of a closed connection (`node state` on SIGHUP) | E2E | `network-test.py::drained` | exists |
| Resident memory levels off under repeated load (second-half peak ≤ first-half peak + 10 %) | Memory plateau | `network-test.py::plateau` | exists, local only |
| Full stores and byte rate refuse requests | Unit | `service/tests/mailbox.rs::full_stores_and_byte_rate_refuse_requests` | exists |

### Deny lists

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Operator deny list refuses a peer or IP group without a strike | Unit | `service/tests/mailbox.rs::deny_list_refuses_without_strikes` | exists |
| Received signed lists are verified and only newer ones kept | Unit | `deny.rs::received_lists_are_checked_and_only_newer_ones_kept` | exists |
| A running node refuses a listed mailbox after SIGHUP, and a listed IP group until it is removed | E2E | `network-test.py::deny_mailbox`, `::deny_group` | exists |
| A shared signed list reaches an accepting node (`dyappd deny received`) | E2E | `network-test.py::shared_deny` | exists |
| Operator CLI reports empty/populated stores without creating missing databases, adds local deny entries idempotently, and displays received lists without applying them | CLI integration | `tests/cli.rs::status_reports_absent_and_populated_stores_without_creating_missing_databases`, `::deny_cli_add_is_idempotent_and_received_lists_are_not_locally_applied` | exists |
| CLI help and incomplete options leave storage untouched | CLI integration | `tests/cli.rs::help_and_incomplete_options_do_not_create_stores` | exists |
| The packaged node reloads a deny entry within 10 seconds; root may not touch the stores | Package | `deb-test.sh` | exists, local only |
| The packaged node is not dumpable and its unit sets `LimitCORE=0` | Package | `deb-test.sh` | exists, local only |
| The packaged node starts from a key piped by `keygen -` into its credential directory; the unit loads `node.key` encrypted | Package | `deb-test.sh` | exists, local only |
| The node key comes from `storage.key`, a credential or `storage.dir`; a credential's mode is not checked, a key file's is | Unit | `config.rs` | exists |
| `dyappd probe-key` validates a stopped copy's key against its peer ID without changing files or opening a listener | CLI integration | `tests/cli.rs::probe_key_checks_a_stopped_copy_without_changing_files` | exists |

### Storage and configuration

| Behaviour | Layer | Test | Status |
|-----------|-------|------|--------|
| Data survives reopen and restart | Unit, E2E | `storage.rs::data_survives_reopen`, `network-test.py::media_retention` | exists |
| A restarted node keeps its peer ID | E2E | `network-test.py::profile_repair`, `::media_retention` | exists |
| SIGTERM stops the node without a handler; SQLite WAL keeps committed data | — | — | accepted: [Decisions](#decisions) |
| The package installs on Debian 12, runs as `dyappd` and removes cleanly | Package | `deb-test.sh` | exists, local only |
| The tarball installs by hand on Debian 12 as in the manual; `status` tells the running node from the stopped one | Package | `tarball-test.sh` | exists, local only |
| Store format steps run once; a newer file is refused | Unit | `storage.rs::store_format_steps_run_once_and_a_newer_file_is_refused` | exists |
| A store format step that fails mid-way (disk full) leaves the previous format and reruns cleanly | Unit | `storage.rs::a_failed_store_format_step_rolls_back_and_reruns` | exists |
| Maintenance frees pages in bounded steps | Unit | `storage.rs::maintenance_frees_pages_in_bounded_steps`, `maintenance.rs` | exists |
| Config precedence file < env < CLI; unknown keys fail; node key bound to the data | Unit, Property | `config.rs` | exists |
| Stopped-node snapshot writer copies required files and SQLite WAL/journal to a private new directory with sorted SHA-256 records; unsafe source paths fail | Python unit | `test_backup_bootstrap.py` | exists; updater integration planned |
| Independent verifier rejects missing/extra files, bad hashes and unsafe manifest paths | Python unit | `test_verify_bootstrap_backup.py` | exists |
| SQLite restore probe copies DB/WAL to a disposable directory, rejects hot journals and corrupt stores without changing the snapshot | Python unit | `test_probe_bootstrap_sqlite.py` | exists |
| Restore preflight checks key against saved peer ID on a separate copy, decrypts age recovery evidence with the pinned real `age` CLI and restores committed WAL data; absent/wrong key or identity, tampered ciphertext, missing `age` executable or corrupt file fails closed | Python unit, Rust CLI | `test_probe_bootstrap_backup.py`, `probe_key_checks_a_stopped_copy_without_changing_files` | exists; updater integration planned |
| Update preflight holds `dyappd.lock`, refuses a running node, too little free space, a missing recovery key, a corrupt store or a key that does not match the saved peer ID, and leaves live storage unchanged | Python unit | `test_preflight_bootstrap_update.py` | exists; updater integration planned |
| Restore holds `dyappd.lock`, re-probes the snapshot, refuses a running node, a corrupt or another node's snapshot and too little space before touching live storage, and swaps in the restored copy while keeping the failed store | Python unit | `test_restore_bootstrap.py` | exists |
| Migrated-store rollback is rehearsed: a new binary migrates and fails to start, restore brings back the snapshot's data and identity, writes after the snapshot are lost and the migrated store is kept aside | Fault test | `test_restore_bootstrap.py::test_failed_migrated_update_is_rehearsed_end_to_end` | exists |

## Decisions

Each accepted gap has a trigger that reopens it.

1. **Mixed versions** wait for the first release tag: there is no older released `dyappd` to run.
   Reopen at the first tag, with that binary in the test image.
2. **coturn allocation** stays local (`make test-integration`); CI runs the loopback suite with the
   TURN credential check only. Reopen when TURN configuration or coturn changes, or a relay fails
   in operation.
3. **Memory under multi-node load and a 24-hour soak** are rejected as too long. The drain check
   (CI) and the 5-minute plateau (local) replace them. Reopen on memory-growth reports from
   operation.
4. **SIGTERM** has no handler: the process ends, SQLite rolls back an open transaction and keeps
   committed data. Reopen if a stop loses committed data.
5. **Time-driven behaviour** (profile expiry, hourly repair, keep expiry) stays covered by unit
   tests and aged fixture rows. A gated test mode with a controllable clock was analysed and
   deferred: no agreed check needs it, and the clock is read in many places.
6. **Functional-layer clients**: `protocol.rs` uses in-process libp2p clients; `dyappd`'s client
   commands are exercised by the network suite. Accepted as enough.

## Sign-off

Accepted by the project owner on 2026-10-12 with the decisions above.
