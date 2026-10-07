# 0001. Record architecture decisions

- **Status:** Accepted
- **Date:** 2026-10-06

## Context

Decisions such as the UniFFI version, Tauri for the desktop shells, and the coverage gate live
only in code and commit messages, with no record of what was decided and why.

## Decision

Keep Architecture Decision Records in `docs/decisions/`, one file per decision, using
[template.md](template.md) and the process in [README.md](README.md). They are written by hand.

## Consequences

- Significant changes should come with an ADR; reviewers can ask for one.
- Existing decisions without ADRs get one when someone touches or questions them.
