# Git Conventions

## Branches

- `main` is the only long-lived branch.
- Work branches: `feature/<topic>`, `bugfix/<topic>`, `docs/<topic>`,
  `chore/<topic>`. Lowercase and hyphens; you may append an issue number
  (`bugfix/42-integration-tests`).
- `main` requires pull requests and green checks. The maintainer has a temporary direct-push
  bypass until the first public release; release hardening removes it and requires signed commits
  ([Threat Model](../security/threat-model.md#ai-agents-and-developer-tooling)).

## Pull requests

External PRs require maintainer approval before CI runs, two independent local AI reviews of a
pinned head SHA, and green checks on that SHA. The maintainer merges with
`gh pr merge --match-head-commit <head-sha>`. Keep infrastructure changes separate from code and
dependency changes. Follow the [review checklist](pr-review.md) and
[ADR 0021](../decisions/0021-ai-review-human-merge.md).

## Commits

[Conventional Commits](https://www.conventionalcommits.org/): `type(scope): Summary`.
The summary starts with a capitalized imperative verb; the local `commit-msg` hook checks it.

- Types: `feat`, `fix`, `docs`, `test`, `build`, `ci`, `refactor`, `perf`, `chore`.
- Scope: a crate (`bootstrap`, `messaging`, `ffi`, …), a platform (`android`,
  `ios`, …) or an area (`security`, `ci`).
- Reference the GitHub issue in the subject or body: `(#42)`.
- Use `WIP:` only for checkpoints whose verification is still pending.
- AI agents add a `Co-Authored-By:` trailer (see [AGENTS.md](../../AGENTS.md)).

## Cargo.lock

Every `Cargo.lock` is committed (`Cargo.lock`, `clients/linux/Cargo.lock`,
`clients/windows/Cargo.lock`) for reproducible builds. Update it only through Cargo,
in the same commit as the `Cargo.toml` change. A shared crate dependency change also updates
both platform lockfiles: the root workspace lock does not cover their separate dependency
graphs. Verify each with `cargo metadata --manifest-path clients/linux/Cargo.toml --locked`
and the equivalent Windows manifest command in the appropriate build environment. Omit
`--no-deps` for this check: it skips dependency resolution and can miss a stale lockfile.

## Rust version (MSRV)

The toolchain is pinned, not "latest stable":

- `rust-toolchain.toml`: `channel = "1.99.0"`
- workspace `Cargo.toml`: `rust-version = "1.99"` (crates inherit it)

Change both together, in a separate `build:` commit, and say why.
