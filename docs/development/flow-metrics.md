# Weekly flow metrics

Practice A7 is a local, read-only report over beads, Git and GitHub Actions.
Run it weekly from the checkout with Python 3.11+, an authenticated `gh`,
and the existing local `bd` database. CI runs its regression tests in
`make quality`; CI never reads or writes beads. No schedule is installed.

```bash
python3 -B tools/scripts/flow-report.py \
  --snapshot-out /tmp/ai/weekly-flow-snapshot.json \
  --report /tmp/ai/weekly-flow-report.json
python3 -B tools/scripts/flow-report.py \
  --snapshot /tmp/ai/weekly-flow-snapshot.json \
  --report /tmp/ai/weekly-flow-replay.json
```

Live collection reads all workflow-run pages, then job pages for relevant
`main` push runs and every rerun attempt, with four concurrent readers.
It retries a failed `gh` command twice. API errors, incomplete pagination,
shallow Git history, a missing pre-week green CI anchor in an older repository,
invalid inputs and threshold breaches exit nonzero with JSON errors.
The command does not fetch Git refs: update local `main` before reporting.
Its snapshot records the local main SHA and GitHub collection counts.
Reports and snapshots contain private bead details; keep them outside Git
and public artifacts. No tracker or GitHub changes are made.

## Daily main health

A weekly report cannot catch a main-red interval before it passes 24 hours, so
check canonical main daily:

```bash
python3 -B tools/scripts/flow-report.py --health --report /tmp/ai/main-health.json
```

It collects only workflow runs (no beads, commits or alerts) and applies the
canonical main rules below to the same seven-day window. The JSON gives
`main_state`, `current_red_hours` and the red intervals that overlap the week.
Exit 1: main red for more than 24 hours, or the data could not be read.
Exit 2: main red for 24 hours or less, or `unknown` (the latest run was
cancelled, is pending or was skipped); unknown is never reported as green.
Exit 0: green. `--snapshot` and `--snapshot-out` work as for the weekly report.

Nothing is scheduled and nothing is sent. To run it daily, a maintainer can add
a user cron entry for a dedicated clone on `main`, such as
`17 7 * * * cd ~/dyapp-health && git pull -q --ff-only && python3 -B tools/scripts/flow-report.py --health`,
or a systemd user timer, and act on a nonzero exit; a red over 24 hours also
needs the A8 postmortem below.

## Definitions

The window is the trailing seven days in UTC, start inclusive and end exclusive.
WIP is the current snapshot, not historical WIP; assignee is the agent identity,
as chosen by the maintainer. Every `in_progress` bead counts, including epics
and expired leases. Unassigned active work is an error. WIP >3 per assignee fails.

Throughput counts currently closed work items with closure in the window,
excluding epics, molecules, convoys and gates. Lead time is creation to latest
closure in hours, not active work time; p50/p85 use nearest rank. Reopened work
is not closed throughput; deleted history and earlier closure cycles cannot be
reconstructed from `bd list`. Empty percentiles and rate denominators are `null`.

Canonical main health is `.github/workflows/ci.yml`, push runs on `main`.
A failed terminal attempt starts red; the next success of that or a newer
generation ends it. Late results from an older generation do not override a newer
green result. Cancellation, pending and skipped results do not prove green; absent an
outstanding red interval, the latest unresolved generation is reported as `unknown`.
Intervals overlapping the week retain their full original age; any such interval
>24 hours fails, even if it recovered during the week. Time-to-green p50/p85
includes intervals recovered during the week. Red-commit rate is unique SHAs with
any failed canonical attempt ending in the week divided by SHAs with a successful
or failed terminal attempt ending in the week; successful retries do not erase failures.

CI minutes sum job execution intervals across all selected main push workflows
and attempts, clipped to the week; parallel jobs add, running jobs stop at snapshot
time. They are execution minutes, not GitHub billing or queue time. Workflow
completion uses the latest job completion, falling back to API `updated_at`
for jobless terminal runs. GitHub retention limits available evidence.

Revert rate counts non-merge main commits in the week with the standard
`This reverts commit` message; manual inverse patches are not identifiable.
Escape hatches count nonempty `Large-Change`, `Size-Increase`, `Docs-Unchanged`
and `Quorum` trailers, plus added `#[mutants::skip]` attributes and leading
`#`/`//` comments containing `gitleaks:allow`. Annotation detection is heuristic;
other spellings and future quarantine systems are not inferred. The report gives
counts by hatch and the share of commits using at least one. More than 10% warns
that A8 follow-up is due; it does not create a bead automatically.

## Explicit observations and reviews

Pass `--ledger /tmp/ai/flow-ledger.json` for classified heuristic evaluations
and completed four-week reviews. For example:

```json
{
  "observations": [
    {"check": "docs-sync", "at": "2026-10-15T10:00:00Z", "false_positive": true}
  ],
  "reviews": [
    {"practice": "A4", "at": "2026-11-15T10:00:00Z", "decision": "keep"}
  ]
}
```

False-positive share is false classifications divided by all explicitly
classified evaluations of that check in the week. Without observations it is
unknown, not zero; record true detections as `false_positive: false` too.
First adoption dates come from committed additions of `(adopted)` rows in
the practice catalogue. Reviews become due after 28 days; an explicit review
on or after that date marks it reviewed. The report lists overdue reviews,
but does not invent outcomes or automatically perform the review.

## Postmortems (A8)

Practice A8. A trigger needs a blameless postmortem within 48 hours: an `analyse`
review of what happened, with each action item filed as its own bead. The review
stays in the bead's notes, with evidence paths and confidence levels; it names
causes and gaps, never people. Triggers, counted from the A8 adoption date:

- `ci-red:<started_at>`: canonical main red for more than 24 hours; the trigger
  time is 24 hours after the red interval started;
- `advisory:<GHSA id>`: a Dependabot alert or a repository security advisory,
  at its creation;
- a regression in a release: planned with the first release (A10).

Record the postmortem as a bead whose external ref is `postmortem:<trigger>`:

```bash
bd create "Postmortem: main red since 2026-10-10" -t task -p 1 -l audit \
  --external-ref "postmortem:ci-red:2026-10-10T17:16:07+00:00"
```

The report lists every trigger in `postmortems` with its due time and bead.
A trigger whose bead is missing after the due time, or was created after it,
is an error. Main red is seen only in the report's seven-day window, so run
the report at least daily while a CI failure is open. Dependabot and advisory
reads need a `gh` token with security-events access.
