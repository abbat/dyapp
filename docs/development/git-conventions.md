# Git Conventions

## Branches

- `main` is the only long-lived branch.
- Work branches: `feature/<topic>`, `bugfix/<topic>`, `docs/<topic>`,
  `chore/<topic>`. Lowercase and hyphens; you may append an issue number
  (`bugfix/42-integration-tests`).
- Small single-maintainer changes (including agent sessions the maintainer
  approves) may be committed directly to `main` until the first public release; from then on
  `main` takes changes only through pull requests with green CI and signed commits (planned,
  [Threat Model](../security/threat-model.md#ai-agents-and-developer-tooling)).

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
in the same commit as the `Cargo.toml` change. A shared crate dependency change also updates
both platform lockfiles: the root workspace lock does not cover their separate dependency
graphs. Verify each with `cargo metadata --manifest-path linux/Cargo.toml --locked`
and the equivalent Windows manifest command in the appropriate build environment. Omit
`--no-deps` for this check: it skips dependency resolution and can miss a stale lockfile.

## Rust version (MSRV)

The toolchain is pinned, not "latest stable":

- `rust-toolchain.toml`: `channel = "1.99.0"`
- workspace `Cargo.toml`: `rust-version = "1.99"` (crates inherit it)

Change both together, in a separate `build:` commit, and say why.
