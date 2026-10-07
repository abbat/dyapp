# Agent Instructions

All project documentation lives in [docs/](docs/README.md); keep new docs there.

## Optional tooling

- **lean-ctx MCP:** if it is installed, read [docs/agents/lean-ctx.md](docs/agents/lean-ctx.md)
  before starting work.
- **Beads (`bd`):** if a local beads database is set up (`bd` is on `PATH` and `.beads/` exists),
  follow [docs/agents/beads.md](docs/agents/beads.md). Otherwise work is tracked in GitHub Issues.

## Build & Test

All targets run in Docker; `make prepare` builds the images once. Main checks: `make test`,
`make quality`, `make test-all`. Full command table, layout and limits:
[docs/agents/project.md](docs/agents/project.md).

## Rules

User instructions take priority over this file. Details:
[docs/agents/project.md](docs/agents/project.md#rules-for-agents).

- No automatic installs and no deletion without explicit user confirmation. Commit and push only
  when the user has authorised it.
- Use non-interactive flags (`cp -f`, `mv -f`, `rm -f`, `rm -rf`, `apt-get -y`,
  `ssh -o BatchMode=yes`): `cp`, `mv` and `rm` may be aliased to `-i` and hang.
- End each commit with the Co-Authored-By line of the agent that made it, e.g.
  `Co-Authored-By: Claude <noreply@anthropic.com>` or `Co-Authored-By: Codex <noreply@openai.com>`.
  The model name may follow the agent name; keep the email unchanged.
- **Docs change with the code.** A new feature, a behaviour fix, or a changed command, config or
  interface updates the affected pages in [docs/](docs/README.md) in the same commit: describe
  what the code does, mark unimplemented parts as planned, delete statements that are no longer
  true, keep the `docs/README.md` index and relative links valid.
