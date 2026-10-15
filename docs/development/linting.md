# Linting

What each linter checks, where it actually runs, and what is only a config file. CI wiring is
described in [ci-base.md](ci-base.md#quality--quality) and
[ci-matrix.md](ci-matrix.md#other-workflows); whether any check is required for merge is **not
established** ([required checks](ci-matrix.md#required-checks)).

## Where each linter runs

| Language | Tool | Config | Quality container / CI `quality` | Other CI | Status |
|----------|------|--------|-----------------------------------|----------|--------|
| Rust | `cargo fmt`, `cargo clippy` | built-in defaults (no `rustfmt.toml`, no `[lints]` table) | yes | `codeql.yml` (clippy, see below) | enforced |
| Python | flake8 | `.flake8` | yes, on `scripts tools tests` | `python-lint.yml`: `flake8 .` | enforced |
| GitHub Actions | actionlint | none | yes | — | enforced |
| Shell | ShellCheck on `scripts/*.sh` | none (defaults) | yes | — | enforced |
| Secrets | gitleaks on the git history | `.gitleaksignore` | yes | — | enforced |
| Commit messages, tracker IDs | `scripts/check-repository.py` ([rules](code-quality.md#commit-messages)) | `BASELINE` in the script | yes | — | enforced after the baseline commit |
| Trigger list | `scripts/triggers.py` ([rules](code-quality.md#trigger-list)) | `scripts/triggers.toml` | yes | — | enforced: every glob matches a tracked file |
| Spelling | typos on the source tree | `_typos.toml` | yes | — | enforced |
| TOML | `taplo fmt --check` | `.taplo.toml` | yes | — | enforced |
| Cargo dependencies | cargo-shear (unused dependencies) | `[package.metadata.cargo-shear]` | yes | — | enforced; cargo-udeps (nightly, compile-precise) is planned |
| Swift | SwiftLint 0.65.1 (`swiftlint lint --strict` on `ios macos`) | `.swiftlint.yml` | yes | — | enforced; three SourceKit rules are skipped on Linux |
| Kotlin | detekt 1.23.8 CLI on `android` | `detekt.yml` | yes | — | enforced; not wired into Gradle |
| Web / JS | — | none | — | — | not configured |

The quality checks are `scripts/rust-check.sh quality`, run in the dev container by `make quality` (also `make lint`,
`make check`, `scripts/check-quality.sh`) and directly on the runner by CI job `ci / core / quality`; the `pre-commit` hook would too, but it
does not run today, see [code-quality.md](code-quality.md#git-hooks).

## Rust

Quality container (the gate):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets --locked --offline -- -D warnings
```

`-D warnings` turns the **default** clippy lint groups (`correctness`, `suspicious`, `style`,
`complexity`, `perf`) into errors. `pedantic`, `nursery` and `restriction` lints (for example
`unwrap_used`, `panic`, `arithmetic_side_effects`) are **not** enabled anywhere in the workspace,
so they are not enforced.

`codeql.yml` runs a separate clippy step with `-W clippy::all -W clippy::pedantic -D warnings`, so
pedantic findings fail that job, and a second clippy run converts its JSON output to SARIF for code
scanning. That job runs outside Docker and is not part of `make quality`; a clean local
`make quality` does not mean it passes.

Run locally: `make quality` (all checks) or `make fmt` (format check only; it does not rewrite files). To reformat, run `cargo fmt --all` in the dev container.

## Python

```bash
python3 -m flake8 scripts tools tests   # quality container
flake8 .                                # python-lint.yml, only when *.py or .flake8 change
```

`.flake8`: `max-line-length = 79`, empty `ignore`, standard excludes (`.git`, `__pycache__`,
`.venv`, `venv`, `build`, `dist`, …). No `max-complexity` is set, so the McCabe check (`C901`) is
off. flake8 only reports; it does not fix anything. `autopep8` or similar is not used by the
project.

## Swift (`.swiftlint.yml`)

The quality step runs `swiftlint lint --strict --quiet --no-cache ios macos` (the source tree is
read-only in the container, so no cache): every warning fails. The binary is the static Linux
build of SwiftLint 0.65.1 (`swiftlint-static` from `swiftlint_linux_amd64.zip`, sha256-checked),
so no Swift toolchain is needed. Without SourceKit
it skips `statement_position`, `vertical_whitespace_opening_braces` and
`vertical_whitespace_closing_braces` and prints a warning for each; on macOS they apply. What the
config sets:

- SwiftLint default rules, plus 57 rules in `opt_in_rules` (including `force_unwrapping`,
  `implicitly_unwrapped_optional`, `missing_docs`, `sorted_imports`, `trailing_closure`). This is
  not "all rules".
- `disabled_rules`: `line_length` (so there is **no** line-length limit), `type_name`,
  `unused_closure_parameter`.
- Thresholds (warning / error): `file_length` 500 / 1000, `type_body_length` 300 / 500,
  `function_body_length` 50 / 100, `cyclomatic_complexity` 10 / 20, `identifier_name` length min
  2 / 1 and max 50 / 60, `large_tuple` 3 / 4, `enum_case_associated_values_count` 5 / 6,
  `nesting` type and function level 2, `vertical_whitespace` max 2 empty lines.
- `trailing_comma: mandatory_comma: true`: a multi-line collection ends with a comma.
- `reporter: xcode`.

`swiftlint --fix` rewrites only rules that support correction; review the diff.

## Kotlin (`detekt.yml`)

The quality step runs the detekt 1.23.8 CLI jar (sha256-checked, on Java 21 in the dev image):
`java -jar /usr/local/lib/detekt-cli.jar --config detekt.yml --input android
--build-upon-default-config`. Any finding fails. `--build-upon-default-config` keeps every
default setting `detekt.yml` does not override. The Gradle build applies no detekt plugin, so
`./gradlew detekt` does not exist. What the config sets: `CyclomaticComplexMethod` threshold 10,
`LongMethod` 30, `LargeClass` 150, `LongParameterList` 5 (functions) / 7 (constructors),
`MaxLineLength` 120; most rule sets active, including the `Undocumented*` comment rules for public
declarations. `FunctionNaming` ignores `@Composable` functions, `MagicNumber` ignores property
declarations, and `FunctionMaxLength` skips the test sources. The `formatting` (ktlint) rule set
is not used.

A detekt **baseline** (`detektBaseline`) records the current findings so they stop failing the
build. It hides them; it does not fix anything. Only add one with an issue to remove the recorded
findings.

## Suppressing a finding

Fix the code first. If a suppression is justified, scope it as narrowly as possible and state why:

```rust
#[allow(clippy::too_many_arguments)] // mirrors the C ABI signature; see #<issue>
fn ffi_entry(/* … */) {}
```

```python
import optional_module  # noqa: F401  (re-exported for callers)
```

A suppression comment only silences the named lint at that spot. It does not satisfy other lints,
and it does not apply to other linters (an `#[allow]` does not affect the `codeql.yml` pedantic run
unless it names that lint too).

## Common fixes

```rust
// Propagate instead of unwrap in library code
let value = option.ok_or(Error::NotFound)?;
```

```python
# E501: wrap long calls instead of disabling the check
result = some_function_with_a_long_name(
    arg1, arg2, arg3,
)
```

```swift
// force_cast
guard let value = object as? String else { return }
```

## Gaps

- `codeql.yml` clippy flags differ from the quality container (pedantic only there).
- Required status checks are not established.
