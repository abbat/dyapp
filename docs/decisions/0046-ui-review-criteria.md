# 0046. Review UI against sourced criteria with automated ratchets

- **Status:** Accepted
- **Date:** 2026-11-07

## Context

The prototype settings screen mixed preferences with data: identities,
devices, stickers, and the blocked and passed lists on one scrolling screen,
with checkboxes for immediate settings. Repeated reviews against Nielsen's
heuristics in [desktop-heuristics.md](../design/desktop-heuristics.md) passed
it. Those heuristics judge interaction one step at a time. They do not judge
where an option lives, which control fits it, or spacing, so a status notice
touching a button row and an identifier touching the hint below it went
unnoticed too.

## Decision

Review every screen against the criteria in
[review-criteria.md](../design/review-criteria.md): settings S1-S8, layout
L1-L2, identifiers I1-I4, and dialogs and text. Each criterion is binary and
cites its source. A failure has severity 3 and blocks the screen.

Automate the mechanical part as ratchets against
`tests/scripts/baselines.toml`. check-repository.py counts the checkboxes and
list rows in each prototype's settings section. The prototype screenshot run
measures the gaps between neighbouring blocks, and check-layout.py applies L2
and I1. Today's findings are the baseline: a new one fails, and a fixed one
must be removed from it. The prototypes are fixed in a separate change.

## Consequences

The checks stop the prototypes from getting worse. They do not prove that a
screen is good. S1, S3, S4, S6-S8, L1, I2-I4 and the dialog criteria still
need a reviewer's judgement. The gap measure is a heuristic with false
positives, such as list rows that touch on purpose. Those stay in the
baseline with a comment, and the baseline grows whenever a reviewed exception
is added. The layout check runs only with the Docker prototype run, not in
the fast repository check. Several criteria (S1-S3, I3) are house rules
because no standard gives a measurable rule for them.
