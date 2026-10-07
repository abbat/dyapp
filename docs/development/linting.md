# Linting

What each linter checks, where it actually runs, and what is only a config file. CI wiring is
described in [ci-base.md](ci-base.md#quality--lintingyml) and
[ci-matrix.md](ci-matrix.md#other-workflows); whether any check is required for merge is **not
established** ([required checks](ci-matrix.md#required-checks)).

## Where each linter runs

| Language | Tool | Config | Quality container / `linting.yml` | Other CI | Status |
|----------|------|--------|-----------------------------------|----------|--------|
| Rust | `cargo fmt`, `cargo clippy` | built-in defaults (no `rustfmt.toml`, no `[lints]` table) | yes | `codeql.yml` (clippy, see below) | enforced |
| Python | flake8 | `.flake8` | yes, on `scripts tools tests` | `python-lint.yml`: `flake8 .` | enforced |
| GitHub Actions | actionlint | none | yes | — | enforced |
| Shell | `bash -n` on `scripts/*.sh` (syntax only) | none | yes | — | enforced |
| Swift | SwiftLint | `.swiftlint.yml` | no | no | config only |
| Kotlin | detekt | `detekt.yml` | no | no | config only, not wired into Gradle |
| Web / JS | — | none | — | — | not configured |

The quality container is `scripts/quality-in-container.sh`, run by `make quality` (also `make lint`,
`make check`, `scripts/check-quality.sh` and `linting.yml`; the `pre-commit` hook would too, but it
does not run today, see [code-quality.md](code-quality.md#git-hooks)).

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

## Swift (`.swiftlint.yml`) — not run

No Makefile target, script, Xcode build phase or workflow runs SwiftLint; the SwiftLint version is
not pinned. What the config sets:

- SwiftLint default rules, plus 58 rules in `opt_in_rules` (including `force_unwrapping`,
  `implicitly_unwrapped_optional`, `missing_docs`, `sorted_imports`, `trailing_closure`). This is
  not "all rules".
- `disabled_rules`: `line_length` (so there is **no** line-length limit), `type_name`,
  `variable_name`, `unused_closure_parameter`. `variable_name` is the old name of
  `identifier_name`; check with `swiftlint rules` whether your version applies the
  `identifier_name` thresholds below.
- Thresholds (warning / error): `file_length` 500 / 1000, `type_body_length` 300 / 500,
  `function_body_length` 50 / 100, `cyclomatic_complexity` 10 / 20, `identifier_name` length min
  2 / 1 and max 50 / 60, `large_tuple` 3 / 4, `enum_case_associated_values_length` 5 / 6,
  `nesting` type and function level 2, `vertical_whitespace` max 2 empty lines.
- `indentation_width: 2` is configured, but `indentation_width` is opt-in and not listed in
  `opt_in_rules`, so the setting has no effect.
- `reporter: xcode`.

To try it on macOS: `swiftlint lint ios/` and `swiftlint lint macos/`. `swiftlint --fix` rewrites
only rules that support correction; review the diff.

## Kotlin (`detekt.yml`) — not run

`detekt.yml` is at the repository root, but `android/build.gradle` and `android/app/build.gradle`
apply no detekt (or ktlint) plugin, so **`./gradlew detekt` does not exist** and no report is
produced. What the config sets: `jvmTarget: "11"`; `ComplexMethod` threshold 10, `LongMethod` 30,
`LargeClass` 150, `LongParameterList` 5 (functions) / 7 (constructors), `MaxLineLength` 120; most
rule sets active. The `formatting` section only takes effect with the separate
`detekt-formatting` plugin.

Wiring it in would mean adding the Gradle plugin with `config = files("$rootDir/../detekt.yml")`
(the file is outside `android/`) and a CI step; that is not done.

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

- SwiftLint and detekt: configs exist, nothing runs them (local or CI).
- `codeql.yml` clippy flags differ from the quality container (pedantic only there).
- Required status checks are not established.
