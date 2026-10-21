# Beads (`bd`)

Applies only when a local beads database is set up (`bd` on `PATH`, `.beads/` present). The
database and `.beads/` are local and not part of the repository; public work is tracked in GitHub
Issues.

The installed binary is the reference: run `bd <command> --help` before using an unfamiliar
command, and `bd prime` for session context. The agent skill is
`.agents/skills/beads/SKILL.md` (Codex, local and not in the repository), shared with Claude
Code through the symlink `.claude/skills/beads` (`/beads`).

## Rules

- Track work in `bd`, not in TODO lists.
- Finish work with `bd close`; there is no `done` status.
- Do not put bead IDs in commits, code or docs: they are local and mean nothing to other readers.
- Write everything you put into beads in English: titles, descriptions, notes, close reasons,
  comments and `bd remember` memories, whatever language the conversation is in.
- `bd dolt push` only when the user has authorised it.
- Label every bead you create (`-l` on `bd create`, `--add-label` on `bd update`); see
  [Labels](#labels).

## Definition of Ready

Practice A1. Before claiming a task, feature or bug bead, check that it is ready, and claim it
through the same script:

```bash
python3 -B tools/scripts/bead-ready.py --claim <id>   # check, then bd update <id> --claim
python3 -B tools/scripts/bead-ready.py <id>...        # check only
python3 -B tools/scripts/bead-ready.py --in-progress  # every claimed bead
```

A bead is ready when it has:

- a goal: a non-empty description;
- at least one verifiable acceptance criterion (`--acceptance`);
- a `Docs:` line naming the pages the work changes, or `Docs: none`;
- a `Verify:` line saying how the result is checked, such as a command or a test.

The two lines may stand in the description, acceptance criteria, design or notes. A bead that is
not ready is completed first (`bd update <id> --append-notes "Docs: ...\nVerify: ..."`), not
claimed. Epics and chores are not checked. The script runs locally only: CI has no beads
database.

## Labels

Use only the labels below; add a new one only after the user agrees, and list it here.

| Label | Meaning |
|-------|---------|
| `feature` | New user-visible or API functionality. |
| `bug` | Wrong behaviour of existing code. |
| `infra` | CI, Docker, build, toolchains, dependencies, repository tooling, tests and test infrastructure. |
| `audit` | Review or check without a planned behaviour change: code, security, versions, docs. |
| `design` | Design work before implementation: architecture, API, protocol, UI/UX, ADR. |
| `docs` | Work whose only output is documentation. |
| `security` | Vulnerabilities, cryptography, privacy, access control. |
| `perf` | Speed, memory, battery, network or storage use. |
| `linux`, `windows`, `ios`, `macos`, `android` | The platform the work touches. |
| `quorum` | Part of a multi-model cross-review; see [quorum.md](quorum.md). |

Each bead gets one kind label (`feature`, `bug`, `infra`, `audit`, `design` or `docs`), plus
`security` and/or `perf` when the work is about that, plus every platform label it touches. Work
that is platform-independent (the shared Rust crates, CI as a whole) gets no platform label.
There is no `test` label: tests are `infra`. `quorum` is added on top of the kind label and is
inherited by every bead under the quorum epic.

## CI failures

`tools/scripts/ci-report-import.py` (run on the host; it needs an authenticated `gh`) turns failing CI
jobs into beads. CI itself never touches beads.

```bash
python3 tools/scripts/ci-report-import.py             # latest completed run of each workflow on main
python3 tools/scripts/ci-report-import.py RUN-ID ...  # specific runs
python3 tools/scripts/ci-report-import.py --dry-run   # print create/note/reopen/skip, change nothing
```

- One bead per failing job, keyed by the external ref `ci:<workflow>/<job>`, type `bug`,
  priority 1, labels `bug,infra`. The aggregate `required` job is skipped when other jobs failed.
- The description holds the run, commit, failed step and the error lines of
  `gh run view --log-failed` (the workflows upload no JUnit reports to parse).
- A later failure of the same job adds a note once per run and reopens a closed bead; importing
  the same run again changes nothing.
- Without run ids only the newest run of each workflow is read, so failures that a later run
  already fixed are not imported.

## Priority proposals

A11 uses the local read-only `tools/scripts/priority-proposal.py`; run it at least every
14 days. It ranks non-closed delivery epics by WSJF = cost of delay / job size,
ties by bead ID, and never changes priorities or dependency readiness.
Use `--practices-first` for the maintainer's current policy: epics labelled
`practices` precede other epics, with WSJF ordering inside each group.
The policy and group membership are bound into the approval fingerprint;
changing policy requires a new approval. A saved proposal is never reordered
in place; `--proposal ... --practices-first` rejects an incompatible policy.

```bash
python3 -B tools/scripts/priority-proposal.py --practices-first \
  --report /tmp/ai/priorities.json
python3 -B tools/scripts/priority-proposal.py --proposal /tmp/ai/priorities.json \
  --approval /tmp/ai/priority-approval.json --check
```

Supply positive finite `metadata.wsjf.cost_of_delay` and `metadata.wsjf.job_size`
and a nonempty `metadata.wsjf.rationale` for an estimated score. Without these,
the proposal explicitly uses provisional proxies: `max(1, 5-priority)` divided by
the number of non-closed descendant leaves (minimum 1). These are estimates,
not measured cost or duration. Quorum participant epics are excluded; nested
delivery epics remain visible and their scores are not independent of their parents.
Review safety constraints, dependencies and estimates before approving an order.

The human approval file contains `approved_by`, timezone-aware `approved_at`
and the exact `proposal_sha256` printed in the proposal. Record it only after the
maintainer explicitly approves. It is an operator attestation, not a signature.
`--check` fails without approval, on an altered proposal/fingerprint, on an
approval for another proposal, or when the proposal is over 14 days old.
Generating a draft exits successfully with `approval: pending`; this never
means the proposed priorities are approved. `--input` reads saved `bd list`
JSON for offline reproduction. Keep these files outside tracked/public files.

## Everyday commands

```bash
bd ready -n 10                       # open, unblocked work
bd show <id>                         # details
python3 -B tools/scripts/bead-ready.py --claim <id>  # check readiness, then claim
bd create "Title" -t task -p 2 --parent <epic-id>
bd dep add <blocked-id> <blocker-id> # <blocked-id> depends on <blocker-id>
bd close <id> --reason "what was done"
bd remember "fact"                   # persistent project memory
```
