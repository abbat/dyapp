# RSS plateau investigation

Status: unresolved. A local release-memory check on 2026-10-15 exceeded its
existing 10% growth allowance. No runtime fix or policy relaxation has been made.

## Measurements

The workload repeats mailbox and media operations on three local release nodes.
One initial round warms caches; the next 300 seconds are sampled after each
round. The gate compares the maximum RSS in the second half of those samples
with the first-half maximum. See [the harness and reports](bootstrap-acceptance.md#layers).

| Observation | Rounds | Node 1 peaks, KiB | Node 2 peaks, KiB | Node 3 peaks, KiB | Result |
|---|---:|---|---|---|---|
| Initial failure | 113 | 28032 → 29896 | 27876 → 31232 | Not printed after node 2 raised | Node 2 +12.04%, failed |
| First retained repeat | 114 | 30168 → 31932 | 28428 → 29884 | 26004 → 28428 | Passed |
| Second retained repeat | 115 | 28028 → 29576 | 27172 → 29004 | 28372 → 29900 | Passed |
| Third series, run 1 | 134 | 31376 → 33588 | 29376 → 31932 | 30540 → 32988 | Passed |
| Third series, run 2 | 128 | 31104 → 35520 | 31136 → 32432 | 30856 → 32268 | Node 1 +14.20%, failed |
| Third series, run 3 | 132 | 33368 → 35184 | 32692 → 35052 | 31212 → 32528 | Passed |
| Third series, run 4 | 132 | 34504 → 37868 | 32176 → 35748 | 35116 → 37596 | Node 2 +11.10%, failed |
| Third series, run 5 | 137 | 31820 → 33116 | 32592 → 33888 | 31684 → 34300 | Passed |
| Third series, run 6 | 131 | 31800 → 34356 | 28656 → 30732 | 30556 → 32496 | Passed |

The third series ran six back-to-back windows on 2026-10-25 with one release binary,
SHA-256 `bf1dd0aaa071b99e97fcdb4d5852b2187ddb810640e7f511d8a3ca666851748d`; two of six
failed, on different nodes. In every run every node's per-quarter maximum rose, by
1–6 MiB over the window, so the gate passes or fails on the size of a steady rise rather
than on noise around a plateau.

The first retained repeat measured 302.43 seconds. Its release binary SHA-256 was
`11a4593d41dfb21e4984e8d70ae3f34de867f3ab084c56a0cbba4ea08f62d906`.
The second retained repeat measured 300.41 seconds and had the same binary hash.
Per-quarter maxima continued increasing on every node in both repeats. Passing
the existing allowance therefore does not establish that allocations stopped growing.
The initial failure lacks a binary hash and complete samples; its unchanged
Rust sources are evidence against a runtime edit in the output-wrapper change,
but cannot establish byte-identical executables across observations.

During a second repeat, two read-only `/proc/<pid>/smaps_rollup` snapshots
showed anonymous memory increasing by 1,956–3,900 KiB per process. Shared clean
pages changed by at most 12 KiB and swap stayed zero. Shared file pages therefore
do not explain most growth in that interval; anonymous RSS still does not
distinguish live allocations from retained allocator pages.

## Causal evidence and remaining questions

- Supported: RSS exceeded the existing allowance in the initial observation.
- Supported by source: the service owns several per-peer limiter maps. Idle
  buckets are pruned when a map reaches 10,000 entries; repeated short-lived
  clients can grow these maps before that boundary (`rate_limit.rs`,
  `PeerRateLimiter::check_units`). This is a bounded-growth candidate.
- Counter-evidence to a simple closed-connection leak: the node removes
  connection maps and watches on close and forgets unrouted trust after the
  last connection (`node/run.rs`, `ConnectionClosed`). This does not prove every
  lifetime or allocation is bounded.
- Hypothesis: SQLite page caches. Nodes open their databases without a
  `cache_size` or `mmap_size` pragma (`storage.rs`, `open`), so each connection
  may cache up to SQLite's default of about 2 MiB, and the workload writes new
  mailbox and media rows every round. A few connections filling their caches
  would match the 1–6 MiB rise; the cache bound would make it level off.
- Unknown: which allocations explain RSS growth. Anonymous memory, retained
  allocator pages, SQLite caches and application maps are competing candidates.
  Existing node-state logs do not expose limiter-map sizes. A passing rerun
  alone cannot distinguish these explanations or demonstrate remediation.

## Next discriminating check

A cheap first check: run the same workload for several windows in one process
and see whether RSS levels off near the sum of the connection cache limits; a
second run with a small `cache_size` should cut the rise if caches explain it.
Retain repeated full-window reports with the same binary and policy, then
correlate anonymous RSS with live map/queue/cache sizes or allocation profiles.
Targeted counters have the advantage of observing existing bounds, but require
review of sensitive runtime paths. Heap profiling can attribute allocations,
but the prepared image lacks the additional tooling and preparation needs
approval. Source-only reasoning is cheap and identifies candidates, but cannot
attribute the measured RSS. Raising tolerance or extending warmup without
measured bounds could conceal a regression and is not a fix.

Any runtime intervention must satisfy the [delivery review gate](../agents/quorum.md#a6-delivery-gate).
Keep the current gate until the mechanism and an alternative acceptance measure
have been reviewed. Verification must include multiple complete observations;
report uncertainty rather than treating one passing window as closure.
