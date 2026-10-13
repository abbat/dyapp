# Code Quality

What is checked, by which command, and what is only a convention. Per-language linter details are
in [linting.md](linting.md).

## Local checks

All checks run in Docker; build the image once with `make prepare`.

| Command | Runs |
|---------|------|
| `make quality` (= `lint`, `check`, `scripts/check-quality.sh`) | `scripts/rust-check.sh quality` (after the container isolation check): `check-repository.py`, actionlint, gitleaks on the git history, typos, `taplo fmt --check`, `shellcheck scripts/*.sh`, Python unittest, flake8, SwiftLint, detekt, `cargo fmt --check`, `cargo shear`, clippy `-D warnings`, `cargo doc` with `RUSTDOCFLAGS=-D warnings` (no tests: `make test`) |
| `make fmt` | `cargo fmt --all -- --check` (check only) |
| `make coverage` (= `scripts/check-quality.sh --coverage`) | cargo-llvm-cov with the 75% workspace line gate |
| `make security` (= `audit`, `deny`) | `rust-check.sh security`: `cargo deny --frozen check advisories bans licenses sources` (offline, the advisory DB from the image) |
| `make doc` | `cargo doc --workspace --no-deps` |
| `make test-all` | `docker-test.sh all`: build, quality, test, coverage, security, network, UI tests |

### Git hooks

`.pre-commit-config.yaml` defines two local hooks:

- `pre-commit` stage: `bash scripts/check-quality.sh` (= `make quality`)
- `pre-push` stage: `bash scripts/docker-test.sh all`

There are no separate rustfmt, YAML, trailing-whitespace or cargo-deny hooks.

Install them with `pre-commit install --hook-type pre-commit --hook-type pre-push`. It refuses to
install while `core.hooksPath` is set (check with `git config core.hooksPath`); in that case run
`make quality` yourself before committing.

## Rust lints

Gate: `cargo clippy --workspace --all-features --all-targets --locked --offline -- -D warnings`,
i.e. clippy's default lint groups as errors. No `[lints]` table, `clippy.toml` or crate-level
`#![deny]` adds more, so pedantic and restriction lints (`unwrap_used`, `panic`,
`arithmetic_side_effects`, `cast_possible_truncation`, …) are **not** enforced by `make quality`.
`codeql.yml` additionally fails on `clippy::pedantic`; see [linting.md](linting.md#rust).

There is no `unsafe` code in `rust/` or `src/`, and no lint forbids adding it. If you add any,
put a `// SAFETY:` comment on each block stating the invariant the caller must uphold.

Formatting: rustfmt defaults (edition 2021, no `rustfmt.toml`).

## Dependency checks (cargo-deny)

`deny.toml`, run by `rust-check.sh security`. Under `make security` it runs offline against the advisory
databases baked into the dev image. In the CI `core / build` step `deny` it fetches them at run time:

- **advisories**: any RustSec advisory fails (no CVSS threshold); a yanked crate fails. Ignored: RUSTSEC-2024-0436
  (`paste` unmaintained, a proc-macro reached only through libp2p's `if-watch`, no upgrade yet).
- **licenses**: only the listed licenses are allowed (permissive set plus `MPL-2.0`); anything else fails.
- **bans**: `openssl` and `openssl-sys` are denied; a second version of a crate fails unless
  `[bans] skip` lists that older version with the dependency that keeps it. cargo-deny warns about
  a skip that no longer matches; remove it then.
- **sources**: crates.io only; an unknown registry or git source fails.

To accept an advisory, add its ID to `[advisories] ignore` with a comment giving the reason and an
issue to remove it. That hides the advisory; it does not fix it.

## Coverage

Threshold, metric, gate locations and the (non-enforced) per-crate targets are defined once in
[Coverage policy](../testing/README.md#coverage-policy): **75% line coverage on the workspace
total**, no per-crate gate. `make coverage` writes `coverage.json`, `coverage.lcov` and
`coverage.txt` (per-file table); find uncovered files there and add tests for the missing
branches.

## Conventions (not enforced by tooling)

- Public items should have doc comments with `# Errors` / `# Panics` sections where they apply;
  `missing_docs` is not enabled, and `make doc` only checks that docs build.
- Library code returns `Result` instead of calling `unwrap`/`expect`/`panic!`; tests may unwrap.
- Do not leave `todo!()` or `unimplemented!()` in committed code; open an issue and return an
  error instead.
- There are no benchmarks (`criterion` is not a dependency and there are no `benches/`).
- Compare secrets (keys, MACs, release secrets) in constant time with `subtle::ConstantTimeEq`,
  never `==`; public values such as nonces, hashes and signatures may use `==`.
- Apps show data from peers (profile text, names, messages) as plain text: no markdown, HTML,
  autolinks or WebView, and no URL scheme or deep link built from it.
- <a id="sql"></a>**SQLite statements.** Quote every table, column, index and alias name with
  backticks; SQL keywords, functions and pragma names stay bare:
  ``"SELECT `record` FROM `profiles` WHERE `peer_id` = ?"``. Backticks keep column names that
  are SQLite keywords (`attachments`.`release`) safe, and one style keeps the queries greppable; SQLite
  accepts them as identifier quotes, as it does the ANSI double quotes, which would need escaping
  in Rust strings. Bind every runtime value as a parameter (`?`, `?1`); never `format!` a value
  into SQL. A `format!` fragment may only be an identifier or clause from an in-code constant
  (`&'static str`), or an integer the code computes where SQLite has no parameter (a `DEFAULT`
  in DDL, a pragma argument). Constant literals written in the SQL itself are fine.

## CI

See [CI matrix](ci-matrix.md) and [CI base](ci-base.md). Job `quality` in `ci-base.yml` runs
`make quality`'s container; job `build` runs build, test, coverage, security and network. Required status checks
are **not established**: see [Required checks](ci-matrix.md#required-checks).

## References

- [Clippy lints](https://rust-lang.github.io/rust-clippy/)
- [cargo-deny](https://embarkstudios.github.io/cargo-deny/)
- [RustSec Advisory Database](https://rustsec.org/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
