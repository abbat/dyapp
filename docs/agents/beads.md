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
- Write everything you put into beads in English: titles, descriptions, notes, close reasons,
  comments and `bd remember` memories, whatever language the conversation is in.
- `bd dolt push` only when the user has authorised it.
- Label every bead you create (`-l` on `bd create`, `--add-label` on `bd update`); see
  [Labels](#labels).

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

Each bead gets one kind label (`feature`, `bug`, `infra`, `audit`, `design` or `docs`), plus
`security` and/or `perf` when the work is about that, plus every platform label it touches. Work
that is platform-independent (the shared Rust crates, CI as a whole) gets no platform label.
There is no `test` label: tests are `infra`.

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
