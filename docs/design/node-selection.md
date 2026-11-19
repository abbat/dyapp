# Sufficient-node selection

**Status: core and mailbox/linked-device/profile/search/media integration are implemented.**
Remaining platform lifecycle integration is pending. This page is both the core behavior contract
and the integration contract; it does not claim full desktop/FFI lifecycle coverage.

## Goals and invariants

The client must bound availability loss from slow or malicious nodes without converging every
client on one fastest node. A sufficient-node set is a health and load-distribution pool, not a
new correctness quorum. It never reduces existing replica, freshness or cross-check coverage.

- Only client-observed transport, progress, completion and validated-response outcomes affect
  health. `info`, roles and other self-reports remain hints subject to normal validation.
- Peer ID proof of work, expected Peer ID, network protocol and response validation run before an
  outcome can improve health.
- Correctness rules remain per operation. A healthy response from the wrong replica or a node
  lacking a required role is not a substitute for the required response.
- Selection, discovery, retries, exploration, connections and outstanding work have fixed bounds.
- No background recovery or exploration traffic is required. Probes reuse real operations.
- Default health history stays in memory. Restart loses reputation rather than creating a durable
  per-network contact history on disk.

## Operation policy

Target and maximum describe the health candidate pool. Required coverage describes the existing
operation contract and takes precedence over weighting.

| Operation | Required correctness coverage | Health target | Health maximum |
|-----------|-------------------------------|---------------|----------------|
| Mailbox write | One accepted holder for every staged replica write; existing R = 5 layout is unchanged | 3 | 5 |
| Ordinary mailbox fetch | One usable holder; another holder on failure | 3 | 5 |
| Mailbox cross-check | Two distinct holders and their union when the persisted check is due | 3 | 5 |
| Profile publish | Existing accepted publication and signed readback; node-side replication is unchanged | 3 | 5 |
| Profile newest read | Highest valid authority across every holder required by the current freshness policy | 3 | 5 |
| Search | Existing independent search coverage, normally two nodes; never reduce it to one | 3 | 5 |
| Media fetch | One holder returning bytes matching the content hash; `NotFound` is not a health fault | 2 | 4 |

Multi-node operations choose without replacement. They do not query every pool member merely to
measure health. For example, search still chooses its required independent responders, while a
freshness read keeps every holder its correctness contract requires.

## Candidate states and outcome accounting

A validated candidate starts with weight 1 and low confidence. Three valid completions establish
normal confidence. Health state has four values:

- **Sufficient:** eligible for ordinary weighted selection.
- **Degraded:** valid but slow; weight 1. It remains usable when healthy alternatives are absent.
- **Circuit open:** not eligible for ordinary selection.
- **Half-open:** eligible only for bounded recovery through a real operation, with weight 1.

Keep separate rolling windows of the last ten hard faults and the last ten slow completions.
Classify outcomes as follows:

| Outcome | Health effect |
|---------|---------------|
| Valid completion within the soft budget | Normal success; reset consecutive hard faults |
| Valid completion after 50% of the operation's total hard budget | Slow success; weight 1, never cancel merely for being slow |
| Honest `NotFound` | Valid response for health; no fault |
| Transport failure on one address | Address fault; try another address without an immediate Peer ID service fault |
| Timeout after applicable addresses or ordinary refusal | Hard Peer ID service fault |
| Proven stale, withheld or otherwise incorrect authority | Correctness fault, counted as a hard fault |
| Malformed, forged or protocol-invalid response | Open the circuit immediately |

Open a circuit after two consecutive hard faults or three hard faults in the ten-outcome window.
Open after three slow completions in the slow window only when at least two other sufficient nodes
have a slow rate below 30%; otherwise keep the slow node degraded and available. This distinction
prevents a trickler from retaining a permanent minimum share while avoiding a false removal of the
only usable high-latency path.

The soft threshold is a health signal, not an additional request timeout. The operation budgets and
whole-request deadlines remain the hard availability bounds.

## Identity, addresses and comparative utility

Track transport health per `(Peer ID, address)` and validated service health per Peer ID. One bad
address must not suppress a healthy alternate address. Once a peer returns malformed, slow or
incorrect service data, that service outcome follows the Peer ID across address rotation.

A fresh Peer ID filling a vacancy in an IP group with an open service circuit remains on probation
until three normal real completions. It cannot immediately replace the open incumbent or acquire an
ordinary weight. Existing healthy members of that group remain eligible. This limits health reset
by ID rotation without treating every user behind one NAT as one malicious node; rotation through
distinct groups remains a residual Sybil risk.

Validity is not utility. A fast empty or partial search response can be valid but less useful than
an independent response containing validated results. When independent responses establish omitted
validated data, three comparative zero-yield outcomes set the node's utility weight to 1 without
opening a hard circuit. Empty results alone are not an attack, media `NotFound` remains honest, and
required independent search/mailbox/profile coverage is unchanged. If every responder censors the
same data, local comparative utility cannot detect it.

## Backoff and recovery

Circuit-open cooldown starts at 60 seconds and doubles after each reopening: 60, 120, 240 and so
on, capped at 1800 seconds. Cooldown uses an injectable monotonic clock.

After cooldown, two consecutive normal, non-slow real operations close the circuit. A fault or slow
completion during half-open reopens it. Recovery probes create no background task and no duplicate
user request.

The backoff episode survives a successful two-probe recovery. It resets only after 64 consecutive
normal, non-slow completions. Otherwise a peer that behaves well for exactly two probes cannot
return to a 60-second cooldown after every new degradation. Weight starts at 1 after recovery and
can rise only after the normal three-observation confidence threshold.

On an observed platform suspend/resume, move every open circuit to half-open at the current
monotonic reading rather than leaving it closed for a long inactive interval. Resume never restores
sufficient state or full weight directly, and a wall-clock jump never closes a circuit. Recovery
still needs the same two normal real operations.

## Weights and load bounds

Track an EWMA of elapsed completion time per operation class and response-size bucket, with alpha
1/4. Compare only the same class and bucket. After three valid observations, derive an integer raw
weight from the current sufficient-pool median:

```text
raw_weight = clamp(floor(4 * median_cost / node_cost), 1, 4)
```

Unknown, degraded and half-open nodes have raw weight 1. Malformed or stale data never improves the
EWMA. An accepted write is a latency observation, not proof of later delivery; receipt and
freshness checks retain their separate correctness effects.

Normalize weights with these hard per-node bounds:

| Sufficient nodes | Maximum share | Minimum share |
|------------------|---------------|---------------|
| 1 | 100% | 100% |
| 2 | 65% | 35% |
| 3–5 | 40% | 10% |

For pools of at least three nodes with multiple IP groups, target an aggregate 50% maximum per
group. This group cap is **best effort**: relax it in ten-percentage-point steps when it conflicts
with per-node caps/floors or a complete allocation. Never fail or loop because the group cap cannot
be satisfied. In particular, a one-node group beside a four-node group needs a 40/60 split. Do not
apply the group cap to a two-node pool.

IP diversity is a preference, not proof of independence. Distinct prefixes can still belong to one
coordinated adversary.

## Bounded exploration

Use a seeded random position inside each operation window:

- one exploration per 20 eligible real operations in a sufficient pool (at most 5%);
- one per 10 operations while below target (at most 10%);
- immediate bounded discovery when the candidate set is empty.

Exploration selects a semantically eligible, normally validated candidate outside the current pool
or a recovered candidate. It routes the original operation instead of issuing an extra probe. Skip
exploration when it would change correctness coverage, disclose a new query solely for health, or
add a second in-flight copy. There is no exploration while the client is idle.

Candidate order is a seeded permutation of the bounded candidate registry, not independent random
draws with replacement. Visit every eligible candidate once before reshuffling for the next cycle.
Independent draws let an attacker flood the candidate set and postpone an honest candidate forever;
the fair permutation gives every retained candidate a finite turn.

At most one unknown or recovering candidate participates in an operation. Hedged requests are off
by default and require a separate design because their traffic and metadata amplification differ.
Production randomness comes from the OS; tests inject a seed or explicit schedule.

## Challenger replacement

Exploration without incumbent replacement cannot escape a full pool of valid work-to-rule nodes.
A responder immediately below the soft threshold opens no circuit, keeps weight 1 and can retain a
large share after normalization even when every honest challenger is faster.

For replacement, retain the actual costs of the last 16 valid observations per operation class and
size bucket. Use the mean of all available observations once the confidence minimum is met; do not
compare a point-in-time EWMA. An adaptive node can place strikes immediately after a check and
recover the EWMA before the next check while retaining a high average cost. The rolling mean sees
every strike.

After three normal valid observations in the same operation class and size bucket, a challenger
replaces the sufficient incumbent with the highest rolling-16 mean cost when:

```text
2 * challenger_rolling_mean <= incumbent_rolling_mean
```

Replacement occurs only on an exploration operation, at most once per exploration, keeps the pool
size constant and preserves the per-node/group bounds and required diversity. The replaced node
remains in the candidate registry without a new fault and may challenge again later. A challenger
that does not meet the cost ratio causes no churn.

A work-to-rule incumbent can avoid this replacement only by keeping its rolling mean below twice the
honest challenger's rolling mean. If it instead approaches the soft budget, a sufficiently faster honest candidate
eventually replaces it. With a stable 128-entry ring and honest candidates retained, three visits to
each of four honest candidates plus four replacements have an upper bound of 7760 operations at the
5% schedule. This is a model bound under those assumptions, not a wall-clock or real-network
availability guarantee.

No replacement rule can provide an escape when discovery contains no honest candidate. A changing
ring, active eclipse or attacker-controlled admission can also invalidate the finite-turn bound.

## Empty, undersized and cold pools

- **Empty:** perform one discovery pass under the end-to-end operation budget. Return the normal
  typed offline/not-found result if no candidate appears; do not retry indefinitely.
- **One node:** use it only for the original required work, expose degraded/partial status where
  the operation already supports it, and issue no duplicate or background work. Attempt rediscovery
  at most once per 60 seconds. Availability of the original operation takes precedence over an
  artificial rate limit, but the client cannot guarantee capacity from one survivor.
- **Below target:** continue required work, use the bounded 10% exploration schedule and avoid
  additional health-only requests.
- **Cold start/restart:** validated candidates begin at weight 1; no historical fastest node gets a
  head start. Per-node bounds still apply.

Keep at most 128 health entries per active network, with bounded per-operation pools. Evict stale
open/degraded entries before sufficient entries and never grow the map merely because an attacker
advertises more Peer IDs. No health entry is persisted by default.

## Privacy and residual risk

Exploration can expose the requested mailbox, profile, search or media interest to one additional
eligible operator. The 5%/10% schedules and real-operation-only rule bound but do not eliminate
that exposure. An empty-pool discovery pass has the same metadata sensitivity as ordinary
discovery. No broadcast health probes or persistent health/contact history are part of this design.

A coordinated swarm across independent IP groups can still dominate discovery. A valid response
just below the soft threshold opens no circuit and can hold a substantial share until challenger
replacement; a response below twice the honest cost can avoid that replacement too, at the price of
a smaller slowdown. Caps and circuits limit concentration and repeated abuse; they do not prove
independence, ensure an honest node exists or find a permanent global optimum. Faster-node caps
sacrifice some peak throughput to avoid herding, and slow-outcome circuits can temporarily penalize
honest high-latency nodes when faster alternatives exist.

An adaptive candidate can answer well only during its three admission observations and strike after
joining; rolling-mean replacement limits this to transient capture rather than preventing admission.
Health is client-local, so a node can behave differently toward different victims. Per-client caps
also do not bound aggregate global load: synchronized clients can oscillate overload between two
honest nodes even though a three-node target can provide more headroom. Finally, fair-ring timing
assumes honest candidates are retained. Replacing the ring before an honest candidate's turn can
postpone exploration forever; no finite escape bound survives uncontrolled admission churn.

## Alternatives

- **Pure round-robin:** simple and even, but deliberately selects known-bad nodes and provides no
  capacity awareness. Rejected as a complete solution.
- **One-best health ranking:** smallest selection state and often lowest latency, but herds clients
  and overloads the winner. Rejected.
- **Weighted choice without exploration:** avoids herding and uses capacity evidence, but can stay
  in a captured or locally optimal set. Insufficient alone; keep bounded exploration.
- **Evolutionary production selection:** a population of node sets can optimize simulated fitness,
  but real fitness observations are noisy, non-stationary, costly and visible to additional
  operators. Crossover has no trust or independence meaning, and selection can herd clients or be
  gamed by an adaptive node. It has no stronger convergence guarantee than the hard fairness and
  replacement bounds here. Rejected as a replacement for those bounds. Evolutionary search is useful
  offline to evolve adversarial schedules against the deterministic model.
- **Hedged requests:** can hide one slow responder, but multiply traffic and metadata exposure.
  Not part of the default policy.
- **Tarpitting a suspected attacker:** holding its request open is beneficial only if that slot
  strictly displaces a new attack and the attacker's marginal cost exceeds the defender's. Current
  asynchronous request handling makes a trickle cheap for the attacker while it occupies client and
  server work; media has 16 global inbound permits, and multiple Peer IDs/IP groups invalidate the
  displacement assumption. Existing whole-request deadlines free bounded resources instead.
  Intentional tarpitting is rejected as a default defence.

## Implementation acceptance

The implementation must provide deterministic tests for cap/floor arithmetic across one to five
nodes, the one-plus-four group case, cold start, a slow-valid responder, a valid responder just below
the soft threshold and evolved strike/recovery schedules that game point EWMA. Cover incumbent
challenger replacement by rolling mean, fair candidate-ring traversal under attacker flooding and
the explicit no-honest-candidate case. Also cover intermittent faults, repeated false recovery,
Peer ID rotation, multiple addresses for one Peer ID, valid-but-unhelpful empty/partial responses,
synchronized load-induced oscillation, suspend/resume clocks, all candidates slow, coordinated
good-then-degraded swarms, heterogeneous honest capacity, one very fast honest node,
insufficient-set transitions and seeded discovery of an unknown or recovered node. Tests use actual
completion signals, controlled clocks and per-transition deadlines, not sleeps. Real protocol tests
must additionally prove that existing mailbox cross-checks, profile freshness and independent
search coverage are unchanged.
