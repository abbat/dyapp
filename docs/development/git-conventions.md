# Git Conventions

## Branches

- `master` is the only long-lived branch.
- Work branches: `feature/<topic>`, `bugfix/<topic>`, `docs/<topic>`,
  `chore/<topic>`. Lowercase and hyphens; you may append an issue number
  (`bugfix/42-integration-tests`).
- Small single-maintainer changes (including agent sessions the maintainer
  approves) may be committed directly to `master`.

## Commits

[Conventional Commits](https://www.conventionalcommits.org/): `type(scope): summary`.

- Types: `feat`, `fix`, `docs`, `test`, `build`, `ci`, `refactor`, `perf`, `chore`.
- Scope: a crate (`bootstrap`, `messaging`, `ffi`, …), a platform (`android`,
  `ios`, …) or an area (`security`, `ci`).
- Reference the GitHub issue in the subject or body: `(#42)`.
- Use `WIP:` only for checkpoints whose verification is still pending.
- AI agents add a `Co-Authored-By:` trailer (see [AGENTS.md](../../AGENTS.md)).

## Cargo.lock

Every `Cargo.lock` is committed (`Cargo.lock`, `linux/Cargo.lock`,
`windows/Cargo.lock`) for reproducible builds. Update it only through Cargo,
in the same commit as the `Cargo.toml` change.

## Rust version (MSRV)

The toolchain is pinned, not "latest stable":

- `rust-toolchain.toml`: `channel = "1.99.0"`
- workspace `Cargo.toml`: `rust-version = "1.99"` (crates inherit it)

Change both together, in a separate `build:` commit, and say why.
