# Code Quality

What is checked, by which command, and what is only a convention. Per-language linter details are
in [linting.md](linting.md).

## Local checks

All checks run in Docker; build the image once with `make prepare`.

| Command | Runs |
|---------|------|
| `make quality` (= `lint`, `check`, `scripts/check-quality.sh`) | `scripts/rust-check.sh quality` (after the container isolation check): `check-repository.py`, `triggers.py`, `docs-sync.py`, actionlint, zizmor, gitleaks on the git history, typos, `taplo fmt --check`, `shellcheck scripts/*.sh`, Python unittest, flake8, SwiftLint, detekt, `cargo fmt --check`, `cargo shear`, clippy `-D warnings`, `cargo doc` with `RUSTDOCFLAGS=-D warnings` (no tests: `make test`) |
| `make fmt` | `cargo fmt --all -- --check` (check only) |
| `make coverage` (= `scripts/check-quality.sh --coverage`) | cargo-llvm-cov with the line coverage ratchet: total ≥ baseline, changed lines ≥80% |
| `make security` (= `audit`, `deny`) | `rust-check.sh security`: `cargo deny --frozen check advisories bans licenses sources` (offline, the advisory DB from the image) |
| `make doc` | `cargo doc --workspace --no-deps` |
| `make test-all` | `docker-test.sh all`: build, quality, test, coverage, security, network, UI tests |

### Commit messages

`scripts/check-repository.py` (in `make quality` and CI `quality`) enforces practice A3 of
[practices.md](practices.md) on every non-merge commit after a baseline commit (`BASELINE` in the
script; older history is grandfathered):

- subject at most 72 characters, capitalised, imperative (no `-ed`, `-ing` or third-person `-s`
  first word after an optional `scope: ` prefix), no trailing period;
- a blank line between the subject and the body;
- a `Co-Authored-By: Name <email>` trailer (not required from `[bot]` authors such as Dependabot);
- no tracker IDs.

Tracker IDs (`dyapp-` followed by 3–4 lowercase letters or digits, optionally `.N`) are also
rejected in every tracked text file. Names declared as `name = "dyapp-…"` in a tracked
`Cargo.toml` are allowed, as is the stale `dyapp-core` in `build-uniffi.sh` (`NOT_TRACKER`).
The check reads committed history, so a bad message fails the next `make quality`; fix it with
`git commit --amend` or a rebase before pushing. A clone without the baseline commit (shallow)
fails the check.

### Docs change with the code

`scripts/docs-sync.py` (in `make quality` and CI `quality`) enforces practice A2: every non-merge
commit after its `BASELINE` that changes an interface line also changes a file under `docs/`.
Interface lines (`RULES` in the script, a heuristic):

- config keys: `pub` fields in `rust/bootstrap/src/config.rs`; `dyappd` derives its
  `--section.key` flags from them;
- CLI flags: `"-…"` strings in `rust/bootstrap/src/bin/*.rs`;
- public API: `pub fn|struct|enum` and `uniffi::` lines in `rust/ffi/src/`, and
  `message`/`enum`/`service`/`rpc` and field-number lines in `proto/*.proto`;
- Make targets: `target:` lines in the `Makefile`.

For a false positive (say, a renamed private config field), add a `Docs-Unchanged: <reason>`
trailer to the commit message. Like the message check, the script reads committed history, so
fix a failing commit with `git commit --amend` or a rebase.

### Trigger list

`scripts/triggers.toml` is the machine-readable list of practice A6a of
[practices.md](practices.md): changes that need extra review. Each trigger has fnmatch `paths`
(`*` also matches `/`) and, optionally, a `lines` regex; then it fires only when an added or
removed line of a matching file matches. The triggers are:

- `crypto`: encryption modules, proof-of-work, endpoint tags;
- `crypto-dependency`: a cryptographic crate line in a `Cargo.toml`;
- `identity`: the `identity` and `profile` crates and their `.proto` files;
- `protocol`: `proto/`, the wire format, protocol IDs, the UniFFI interface;
- `storage-migration`: a `CREATE`/`ALTER`/`DROP` `TABLE`/`INDEX` or `user_version` line in Rust;
- `unsafe`: an `unsafe` line in Rust;
- `network-surface`: `p2p-net` and the bootstrap node, service, rate limits, deny list,
  replication and configuration;
- `checks`: workflows, scripts, tests, Docker images, lint and deny configurations, and this list.

`python3 scripts/triggers.py` (in `make quality` and CI `quality`) fails when a glob matches no
tracked file, so a moved or deleted path cannot silently drop out of the list. With `git diff`
arguments it also prints the triggers a change fires, one `trigger: path` per line:

```sh
python3 scripts/triggers.py origin/main...HEAD   # a branch
python3 scripts/triggers.py --cached             # staged changes
```

The detector only reports; the review a fired trigger requires (a quorum, a threat model update)
is planned as practices A6 and C1. Removing or narrowing an entry weakens a check, and editing the
list fires `checks` itself.

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
i.e. clippy's default lint groups as errors. Apart from the unsafe policy below, no `[lints]`
table, `clippy.toml` or crate-level `#![deny]` adds more, so pedantic and restriction lints (`unwrap_used`, `panic`,
`arithmetic_side_effects`, `cast_possible_truncation`, …) are **not** enforced by `make quality`.
`codeql.yml` additionally fails on `clippy::pedantic`; see [linting.md](linting.md#rust).

### Unsafe policy

`unsafe_code = "forbid"` applies to every crate except two:

- workspace members via `[workspace.lints.rust]` in the root `Cargo.toml` and `[lints] workspace =
  true`; `linux/`, `windows/` and `tools/arch-lint` set it in their own `[lints.rust]`;
- `rust/bootstrap` (libc calls) and `rust/ffi` (uniffi generates `#[no_mangle]`) instead deny
  `clippy::undocumented_unsafe_blocks`: each `unsafe` block needs a `// SAFETY:` comment stating
  the invariant that makes it sound.

`scripts/check-repository.py` fails when a crate's `Cargo.toml` lacks its policy, so a new crate
cannot skip it. Adding `unsafe` code is a change trigger (see [Trigger list](#trigger-list)).

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
[Coverage policy](../testing/README.md#coverage-policy): the workspace line total never drops
below the baseline in `scripts/baselines.toml`, lines changed since the base are ≥80% covered, and
there is no per-crate gate. `make coverage` writes `coverage.json`, `coverage.lcov` and
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
