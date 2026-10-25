# Search capacity

> **Status: done.** This page checks whether the agreed profile search design
> ([bootstrap.md, Search](bootstrap.md#search)) holds at scale: the method, the model, the SQLite
> measurement and the [results](#results).

The check covers the current design only: every search node collects the whole network up to
`search.max_bytes`, or a uniform sample when it subscribes to some topics. If it fails at a scale
step, a separate task changes the mechanism; this page records where and why.

## Inputs

All inputs live in one parameter block of the model and can be changed.

| Input | Values |
|-------|--------|
| Registered users | 1K, 10K, 100K, 1M, 10M, 50M (top step ≈ 15M DAU) |
| Devices per user | 1.5 |
| Base activity | 30 % DAU, 10 queries per active user a day, profile edit once a week |
| Stress activity | 50 % DAU, 30 queries per active user a day, profile edit once a day |
| Evening peak | 3 × the daily average |
| Heartbeat | once a day per active user |
| Bootstrap nodes | 0.1 %, 0.5 %, 1 % of DAU, at least 1 |
| Share running search | 10 %, 50 %, 100 % |
| Node | 2 vCPU, 4 GB RAM, 80 GB SSD, 100 Mbit/s; `search.max_bytes` = 50 GB |
| Profile | 5 KB average, 64 KiB maximum, plus index overhead (measured) |
| Gossip | 16 topics, D = 6, hourly digest catch-up, inventory in batches of 10 000 |
| Places | Zipf within a country: largest place ≈ 9 %; countries Zipf, largest ≈ 30 % |
| Filter selectivity | gender and orientation ≈ 45 %, age ± 5 years ≈ 25 %, goal ≈ 40 %, rare filter (education, kids) ≈ 5–10 % |

Query classes: **place only**; **typical** (place + gender + age, ≈ 11 % of the place);
**rare** (≈ 1 % of the place); **country + rare** (no place, ≈ 0.25 % of the country);
**place IN** (8 places + typical filter).

## Model

`tests/scripts/search-model.py` (stdlib only, runs in seconds; `--bench <json>` takes the measured
bytes per row, query capacity and `partial` share, otherwise it uses 300 B per row and 500 queries
per second and says so) computes for every step × activity × node count × search share:

| Output | Computed as | Holds when |
|--------|-------------|------------|
| Disk | users × (profile + index row) | ≤ `search.max_bytes`; above it, the share of the network a node keeps |
| Search nodes | bootstrap nodes × search share | ≥ 2 (a client publishes to 2); the report gives the DAU at which the shares reach 2–3 nodes |
| Gossip in | (edits + heartbeats) × copies 1 / 3 / 6 | ≤ 30 % of the link (≈ 4 MB/s) at 6 copies; holding only at 3 is reported as depending on gossipsub IDONTWANT |
| Inventory | worst case: the whole inventory hourly in 10 000-key batches (a partial sample differs in most buckets); a node in sync exchanges ≈ 8 KB per topic digest | ≤ 10 % of the link |
| First sync | a new or long-offline node fills its index at 30 % of the link | ≤ 24 h; it answers `partial` meanwhile; the load on the peers it pulls from is reported |
| Queries per node at peak | DAU × queries × 3 / search nodes | ≤ 50 % of the measured SQLite capacity |
| `get` per node at peak | queries × 50 × 5 KB | ≤ 30 % of the link |
| Place coverage | expected matches on one node for Zipf place ranks under a partial sample | ≥ 50 (one page); else the client asks another node |
| Nodes per query | nodes needed for a full page in a small place or with a rare filter | ≤ 3 |
| `partial` | share of queries that exhaust `max_scan_rows` = 10 000 before 50 matches | ≤ 5 % |

The model prints a verdict and the limiting resource per step, the DAU at which the design stops
holding, and how `search.max_bytes`, D, heartbeat rate and `max_scan_rows` move that threshold.
A `country + rare` query that is always `partial` is reported as expected (the client pages or asks
more nodes), not as a failure.

The [adversary section](#adversary) contrasts the old unprotected scenario with the
implemented newcomer quota, largest-place eviction, heartbeat work and query puzzle.
It reports fake shares, honest growth pressure, rare-place coverage and attacker costs;
hardware assumptions and accepted residual risks are stated explicitly.

`--self-check` asserts the model against hand-computed numbers (1M users, base: ≈ 5 GB of full
records, ≈ 0.01 MB/s of edits), that disk and traffic do not shrink as the step grows, and that the
Zipf place shares sum to 1; it runs before the report. Used alone, this check
prints `OK search-model` or mapped failure details (`--raw --self-check` keeps
legacy diagnostics). The default capacity tables remain a report data mode.

## SQLite measurement

`tests/scripts/search-bench.py --rows 100000 --rows 1000000 --rows 2500000 --dir <db dir> --out
<json>` (stdlib `sqlite3`) runs once by hand in the dev container limited to
`--cpus 2 --memory 4g`, within 10 minutes and at most 10 GB of disk; past the budget it writes the
sizes done so far with `partial_run`. `<db dir>` must hold ≈ 1.5 GB, more than the container's
`/tmp`. A node at 50 GB keeps ≈ 10M profiles; that size is extrapolated, not built:

1. Generate a synthetic `profiles-idx.db` with the schema from bootstrap.md (typed columns,
   interests table, random `r`) at 100K, 1M and 2.5M rows (`--rows`), with the place and filter
   distributions above. Rows are generated inside SQLite (`INSERT … SELECT` from a recursive CTE
   over small place-weight tables); the indexes (country, place, r), (country, r), (r) are built
   after the insert. The script stops before generating if the expected file exceeds 10 GB or the
   free space.
2. Run 1 000 queries for each query class × place class (top place, rank ≈ 100, rank ≈ 10 000),
   in the design's form `WHERE … AND r ≥ s ORDER BY r LIMIT 50` with wrap-around and
   `max_scan_rows` = 10 000 (an inner `ORDER BY r LIMIT 10000` over the scope, the filter
   outside it), and record `EXPLAIN QUERY PLAN` per class: the index the planner picks and
   whether it sorts in a temporary B-tree.
3. Record p50/p99 latency, queries per second on 2 threads, `partial` share, rows scanned, index
   file size and RSS as JSON. The model fits latency as log N plus rows scanned on 100K and 1M,
   checks the fit against 2.5M, and calls the extrapolation to 10M unreliable if the 2.5M error
   exceeds 30 %; bytes per row come straight from the measured sizes.

Python's `sqlite3` is not the rusqlite bundled build; plans and orders of magnitude carry over,
exact timings may not. The scripts are a one-off study: no CI job, no Make target.

## Results

Measured on 2026-10-12: SQLite 3.46.1, 100K + 1M + 2.5M rows in 232 s, `--cpus 2 --memory 4g`.
The model ran with that JSON. **The design holds at every step up to 50M users** in all 18
activity × node share × search share combinations; no parameter in bootstrap.md changes. Its
limits are at the bottom of the scale (too few search nodes), in the long tail of places at 50M,
in the `place IN` query plan and against an adversary.

### SQLite

| Size | File | Bytes per row | RSS | Build | Typical qps, 2 threads |
|------|------|---------------|-----|-------|------------------------|
| 100K | 45 MiB | 472 | 72 MB | 1 s | 2 575 |
| 1M | 441 MiB | 462 | 556 MB | 14 s | 1 312 |
| 2.5M | 1 100 MiB | 461 | 1.0 GB | 41 s | 1 079 |

At 2.5M rows, place rank 1 / 100 / 10 000 of the largest country:

| Class | Plan | p50 / p99, ms | Rows scanned | Matches | `partial` |
|-------|------|---------------|--------------|---------|-----------|
| place only | covering (country, place, r) | 0.03 / 0.06 | 50 | 50 | 0 |
| typical, rank 1 | (country, place, r) | 1.34 / 2.07 | 463 | 50 | 0 |
| typical, rank 100 | (country, place, r) | 0.51 / 0.84 | 460 | 50 | 0 |
| typical, rank 10 000 | (country, place, r) | 0.04 / 0.07 | 29 (whole place) | 3.1 | 0 |
| rare, rank 1 | (country, place, r) | 13.7 / 251 | 5 051 | 50 | 0 |
| rare, rank 100 | (country, place, r) | 0.78 / 1.02 | 720 (whole place) | 6.4 | 0 |
| country + rare | (country, r) | 30.7 / 326 | 10 000 | 22.5 | 1.0, expected |
| place IN, 8 places | (country, place, r) + **temporary B-tree** | **65.7 / 445** | all rows of the 8 places | 50 | 0 |

The typical p50 grows as log N plus rows scanned; the fit on 100K and 1M is 25 % off at 2.5M
(within 30 %), so the 10M extrapolation holds: ≈ 1.3 ms, ≈ 1 079 queries per second warm. A
10M index (≈ 4.6 GB) exceeds half the node's RAM, so the model also counts a cold cache:
≈ 455 page reads × 0.1 ms ≈ 44 queries per second.

**`place IN` sorts.** SQLite merges the 8 index ranges in a temporary B-tree: it reads and sorts
every row of the 8 places before `LIMIT`, so `max_scan_rows` does not bound it and the cost grows
with the places (50 × typical at p50). The fix is in the node, not in the protocol: one ordered
scan per place merged by `r`, each capped by the shared scan budget.

### Model

The threshold is set by search nodes, not by load: a step holds once at least 2 search nodes
exist. That is from 1K users (1 % of DAU run bootstrap, all of them search) to 100K (0.1 %, 10 %);
2 search nodes need 200–20 000 DAU depending on the shares. Above it, with base activity,
0.5 % nodes and 50 % search:

| Users | Search nodes | Disk per node | Sample | Queries per node at peak | Nodes for a page: typical r1 / r100 / r10k | Rare r1 / r100 / r10k |
|-------|--------------|---------------|--------|---------------------------|---------------------------------------------|-----------------------|
| 100K | 75 | 0.55 GB | 1.00 | 0.14/s | 1 / 1 / 1 | 1 / 1 / 1 |
| 1M | 750 | 5.5 GB | 1.00 | 0.14/s | 1 / 1 / 1 | 1 / 1 / 1 |
| 10M | 7 500 | 50 GB | 0.92 | 0.14/s | 1 / 1 / 2 | 1 / 2 / 2 |
| 50M | 37 500 | 50 GB | 0.18 | 0.14/s | 1 / 1 / 15 | 1 / 2 / 15 |

The tightest case (stress activity, 0.1 % nodes, 10 % search: 2 500 search nodes at 50M) peaks at
10.4 queries per second per node, under the 44 of a cold cache; gossip at 6 copies reaches
3.3 MB/s of the 4 MB/s budget and `get` 2.6 MB/s of 3.75. First sync of a full node takes ≈ 3.7 h.
Halving or doubling `search.max_bytes`, D = 3 or 12, 2 heartbeats a day and `max_scan_rows`
5 000 or 20 000 do not move the threshold.

### Adversary

Without the defences below, at the historical 5 ms/hash assumption one 16-core PC
creates ≈ 135,000 fake keys/day at 11 bits, filling 50 GB in ≈ 70 days with the
model defaults. Oldest-heartbeat eviction would then lose one real profile per fake.
At 14 bits a phone takes 240 s, beyond the 30 s target; PoW alone is insufficient.
Likewise 50 peers × 100 requests/s offer 5,000/s, 10 times the default warm 500/s
capacity. The 1,000/s IP-group cap does not bound an attacker spread across groups.

Defences, implemented on the node:

- **Canonical places.** `place` becomes a GeoNames ID from cities1000 (population ≥ 1 000, CC BY
  4.0), checked against a set embedded in the node; an unknown or missing place is indexed in the
  country bucket (`place = ""`), so invented rare places buy nothing.
- **Newcomer quota.** A key is a newcomer for `search.newcomer_days` (30) after a node first sees
  it; newcomers hold at most `search.newcomer_share` (10 %) of `search.max_bytes`, and a new key
  past the quota evicts the newcomer with the oldest heartbeat. Keys a new node receives from
  other search nodes in its own first 30 days count as established, so it can fill from its
  neighbours. The `node status` log line and `dyapp_search_newcomer_evictions_total` count
  newcomer evictions.
- **Water-filling eviction.** A full node evicts established keys from the place with the most
  profiles, highest random rank `r` first, so each node keeps its own subsample of big places and
  small places stay whole; the country bucket goes first. Tombstones leave by TTL only. If 16
  places in a row hold only newcomers, eviction falls back to all established keys by `r`.
- **Heartbeat proof of work.** A search heartbeat carries an Argon2id proof for its UTC day
  (`search.min_heartbeat_pow_bits`, 8 by default, ≈ 4 s on a phone once a day), so keeping a
  fake alive costs work every day (implemented on the node, client solving planned;
  [bootstrap.md](bootstrap.md#search)).
- **Fair queue.** `query` and `get` wait in one queue per IP group (/24, /48): at most 4 per
  group, 256 in total, served round-robin; a full queue answers `RATE_LIMITED`.
- **Load puzzle.** When the median queue wait passes `search.puzzle_wait_ms` (100), a query or
  get must carry a SHA-256 solution bound to the peer ID and a challenge rotated every minute;
  difficulty climbs from 12 to `search.max_puzzle_bits` (20) and falls back to off when load
  drops. The status `PUZZLE` carries the challenge (implemented on the node, client solving
  planned; [bootstrap.md](bootstrap.md#search)).

Accepted residuals: established fakes, false claims about real rare places, competition with
honest newcomers, the catch-up grace window, and fast or distributed puzzle solvers
([threat model](../security/threat-model.md#denial-of-service)).

#### Defence model and self-check

Run `python3 -I tests/scripts/search-model.py` (assertions always run first), or add
`--self-check` for assertions only. The old adversary table is a counterfactual without
the implemented defences, retaining its original 5 ms/hash assumption.

Defaults: 50 GB, equal-size 5 KB records plus 300 B index overhead, 30 days, 10 % newcomer
record-byte quota, 8-bit heartbeat. The quota admits 1,000,000 fresh fakes if they alone
occupy it; honest newcomers reduce this room. It is **not** a 10 % count limit:
with 1M honest profiles an otherwise unbounded index is 50 % fake. On a full node,
including index overhead and the 95 % low-water mark, newcomer fakes are about **11.16 %**
by count. Smaller fake records increase that count; established fakes and grace imports
are outside the bound.

Water-filling solves `sum(min(place_size, ceiling)) = established_capacity`, reserving
the entire newcomer quota and the 95 % low-water mark. This scenario assumes countries
have the same within-country Zipf shape and capacity proportional to population (largest
country 30 %). It is an ideal steady state, not an eviction simulation; variable record
sizes, churn, country-only profiles and fallback after 16 newcomer-only places can change
it. Without established fakes concentrated in the rare place, rank 10000 is held whole
on **one node** at all model steps (target ≤ 3).

| Users | Rank-10000 profiles | Established ceiling/place | Rare place kept | Nodes for whole place | Quota used by 0.1 / 0.5 / 1 % daily growth |
|-------|--------------------|---------------------------|-----------------|-----------------------|------------------------------------------|
| 1M | 3.5 | 27,000.0 | 100 % | 1 | 3 / 15 / 30 % |
| 10M | 35.1 | 21,041.9 | 100 % | 1 | 30 / 150 / 300 % |
| 50M | 175.6 | 252.2 | 100 % | 1 | 150 / 750 / 1500 % |

Growth is a constant arrival rate relative to the listed population, with no departures
during 30 days. The quota binds above **33,333 new keys/day**, even without an attacker:
0.1 %/day fits at 10M but not 50M; 0.5 %/day already fails at 10M. These are sensitivity
assumptions, not observed adoption. Size eviction removes established keys only, so
overflow does not repeatedly halve the newcomer pool (R5); quota pressure and TTL can
still evict newcomers before they mature.

Heartbeat measured on 2026-10-19: release `dyapp_profile::heartbeat_pow_bits`, Argon2id
8 MiB, one pass, one lane, 32-byte output; x86_64 Docker with one CPU; 20 warm-up calls
then 1000 sequential hashes: **2.674 ms/hash**. To reproduce, build `dyapp-profile`
offline in release mode, then time 1000 calls with key `[5; 32]`, day 20000 and nonces
0–999 through `std::hint::black_box`, using `std::time::Instant`; divide elapsed
milliseconds by 1000. Hardware and memory contention affect this value; extrapolation
to 16 cores assumes optimistic linear scaling.

At 8 bits, 256 expected hashes cost **0.684544 core-s/fake/day** for one proof daily.
N fakes cost `N × 0.684544 / 3600` core-hours/day: **190.2 core-hours/day for 1M**;
16 dedicated cores sustain about **2.02M**, excluding initial profile proofs and other
work. Daily attendance is a scenario, not enforced: a retained key can heartbeat less
often within the TTL, reducing average cost. Becoming established does not prove humanity.

Puzzle rates below assume 1M SHA-256 trials/s for CPU and 10B/s for GPU: hardware
scenarios, not measurements. Requests/s = trials/s / 2^bits, before queue and rate limits.

| Solver | 12 bits requests/s | 20 bits requests/s | One warm fair share, 50 active groups |
|--------|--------------------|--------------------|--------------------------------------|
| CPU | 244.141 | 0.954 | 10 requests/s |
| GPU | 2,441,406.250 | 9,536.743 | 10 requests/s |

Warm capacity 500/s divided by 50 backlogged IP groups gives 10/s per group; cold 44/s
gives 0.88/s. This illustrates equal-cost reads, not a latency guarantee. At 20 bits
the CPU scenario drops below one fair share; fast hardware still needs the queue.
Multiple attacker groups acquire multiple shares.

Assertions cover quota, heartbeat and puzzle hand calculations, water-filling with empty,
fitting and overflowing inputs, and a reduced-capacity case losing rare-place coverage.

### Answers

- **Users per place and selectivity.** On one node at 10M users the largest place holds ≈ 250 000
  profiles and rank 100 ≈ 2 800; the typical filter (≈ 11 %) leaves ≈ 27 000 and ≈ 310 matches.
  A page then costs ≈ 460 rows scanned; a rare filter (≈ 1 %) ≈ 5 000; country + rare
  (≈ 0.25 %) exhausts 10 000 rows and is `partial`, as designed.
- **Are 1–3 nodes enough?** The historical uniform-sample model needed ≈ 15 nodes for
  rank 10000 at 50M. With largest-place eviction, the ideal established-profile model
  above keeps that place whole on one node; this assumes it is not itself flooded.
- **Small places and rare filters.** A small place is read whole through (country, place, r) in
  well under 1 ms and never `partial`; a page is short because the place is. A rare filter in a
  large place stays under `max_scan_rows` (p99 ≈ 250 ms). Country-wide rare filters are
  `partial`, and the client asks again.
- **A sharded index for hot places?** No: the measured largest place costs 1.3 ms per
  typical query and peak load is ≈ 10 queries/s per node. The model above addresses
  long-tail coverage by largest-place eviction rather than partitioning.

Follow-up work, tracked separately: `place IN` as a per-place merge and the client side
of the defences in [Adversary](#adversary). The defence model above now includes
water-filling coverage of long-tail places at 50M users.
