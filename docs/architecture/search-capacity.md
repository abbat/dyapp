# Search capacity

> **Status: planned.** This page states the method for checking whether the agreed profile search
> design ([bootstrap.md, Search](bootstrap.md#search)) holds at scale. The model, the SQLite
> measurement and the results below are not written yet; numbers here are inputs and thresholds,
> not findings.

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
| Gossip | 16 topics, D = 6, hourly inventory in batches of 10 000 |
| Places | Zipf within a country: largest place ≈ 9 %; countries Zipf, largest ≈ 30 % |
| Filter selectivity | gender and orientation ≈ 45 %, age ± 5 years ≈ 25 %, goal ≈ 40 %, rare filter (education, kids) ≈ 5–10 % |

Query classes: **place only**; **typical** (place + gender + age, ≈ 11 % of the place);
**rare** (≈ 1 % of the place).

## Model

`scripts/search-model.py` (planned; stdlib only, runs in seconds) computes for every
step × activity × node count × search share:

| Output | Computed as | Holds when |
|--------|-------------|------------|
| Disk | users × (profile + index row) | ≤ `search.max_bytes`; above it, the share of the network a node keeps |
| Gossip in | (edits + heartbeats) × up to D copies | ≤ 30 % of the link (≈ 4 MB/s) |
| Inventory | hourly exchange in 10 000-key batches | ≤ 10 % of the link |
| Queries per node at peak | DAU × queries × 3 / search nodes | ≤ 50 % of the measured SQLite capacity |
| `get` per node at peak | queries × 50 × 5 KB | ≤ 30 % of the link |
| Place coverage | expected matches on one node for Zipf place ranks under a partial sample | ≥ 50 (one page); else the client asks another node |
| Nodes per query | nodes needed for a full page in a small place or with a rare filter | ≤ 3 |
| `partial` | share of queries that exhaust `max_scan_rows` = 10 000 before 50 matches | ≤ 5 % |

The model prints a verdict and the limiting resource per step, the DAU at which the design stops
holding, and how `search.max_bytes`, D, heartbeat rate and `max_scan_rows` move that threshold.

## SQLite measurement

`scripts/search-bench.py` (planned; stdlib `sqlite3`) runs once by hand in the dev container
limited to `--cpus 2 --memory 4g`, within 10 minutes:

1. Generate a synthetic `profiles-idx.db` with the schema from bootstrap.md (typed columns,
   interests table, random `r`, indexes (country, place, r), (country, r), (r)) at 10M rows —
   the profiles one node keeps at 50 GB — with the place and filter distributions above.
2. Run 1 000 queries for each query class × place class (top place, rank ≈ 100, rank ≈ 10 000),
   in the design's form `WHERE … AND r ≥ s ORDER BY r LIMIT 50` with wrap-around and
   `max_scan_rows` = 10 000.
3. Record p50/p99 latency, queries per second on 2 threads, `partial` share, rows scanned, index
   file size and RSS as JSON; the model reads query capacity and bytes per index row from it.

Python's `sqlite3` is not the rusqlite bundled build; plans and orders of magnitude carry over,
exact timings may not. The scripts are a one-off study: no CI job, no Make target.

## Results

Planned. This section will give the per-step table, the measured numbers, the threshold, and
answers to: users per place and filter selectivity in practice; whether asking 1–3 nodes is
enough; how small places and rare filters are found; whether hot places need a dedicated sharded
index. Parameter changes that keep the mechanism go into bootstrap.md in the same commit.
