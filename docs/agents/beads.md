# Beads (`bd`)

Applies only when a local beads database is set up (`bd` on `PATH`, `.beads/` present). The
database and `.beads/` are local and not part of the repository; public work is tracked in GitHub
Issues.

The installed binary is the reference: run `bd <command> --help` before using an unfamiliar
command, and `bd prime` for session context. The agent skill is
[`.agents/skills/beads/SKILL.md`](../../.agents/skills/beads/SKILL.md) (Codex), shared with Claude
Code through the symlink `.claude/skills/beads` (`/beads`).

## Rules

- Track work in `bd`, not in TODO lists.
- Finish work with `bd close`; there is no `done` status.
- Do not put bead IDs in commits, code or docs: they are local and mean nothing to other readers.
- `bd dolt push` only when the user has authorised it.

## Everyday commands

```bash
bd ready -n 10                       # open, unblocked work
bd show <id>                         # details
bd update <id> --claim               # assign to you, status in_progress
bd create "Title" -t task -p 2 --parent <epic-id>
bd dep add <blocked-id> <blocker-id> # <blocked-id> depends on <blocker-id>
bd close <id> --reason "what was done"
bd remember "fact"                   # persistent project memory
```
