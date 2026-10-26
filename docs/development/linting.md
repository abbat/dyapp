# Linting

What each linter checks, where it actually runs, and what is only a config file. CI wiring is
described in [ci-base.md](ci-base.md#quality--quality) and
[ci-matrix.md](ci-matrix.md#other-workflows); whether any check is required for merge is **not
established** ([required checks](ci-matrix.md#required-checks)).

## Where each linter runs

| Language | Tool | Config | Quality container / CI `quality` | Other CI | Status |
|----------|------|--------|-----------------------------------|----------|--------|
| Rust | `cargo fmt`, `cargo clippy` | built-in defaults (no `rustfmt.toml`); Cargo `[lints]` for [unsafe](code-quality.md#unsafe-policy), docs and suppression policy | yes | `codeql.yml` (clippy, see below) | enforced |
| Python | flake8 | `.flake8` | yes, on `scripts tools tests` | `python-lint.yml`: `flake8 .` | enforced |
| GitHub Actions | actionlint | none | yes | — | enforced |
| GitHub Actions security | zizmor (offline audits) | `.github/zizmor.yml` | yes | — | enforced |
| Shell | ShellCheck on `tests/scripts/*.sh`, `tools/scripts/*.sh`, `tools/build-scripts/*.sh` | none (defaults) | yes | — | enforced |
| Secrets | gitleaks on the git history | `.gitleaksignore` | yes | — | enforced |
| Commit messages, tracker IDs | `tests/scripts/check-repository.py` ([rules](code-quality.md#commit-messages)) | `BASELINE` in the script | yes | — | enforced after the baseline commit |
| Source file size | `tests/scripts/check-repository.py` ([rules](code-quality.md#size-budgets)) | `[size]` in `tests/scripts/baselines.toml` | yes | — | 1000 lines; older files budgeted |
| Trigger list | `tests/scripts/triggers.py` ([rules](code-quality.md#trigger-list)) | `tests/scripts/triggers.toml` | yes | — | enforced: every glob matches a tracked file |
| Docs with the code | `tests/scripts/docs-sync.py` ([rules](code-quality.md#docs-change-with-the-code)) | `RULES` in the script | yes | — | enforced: commits after the baseline |
| Spelling | typos on the source tree | `_typos.toml` | yes | — | enforced |
| TOML | `taplo fmt --check` | `.taplo.toml` | yes | — | enforced |
| Cargo dependencies | cargo-shear (unused dependencies) | `[package.metadata.cargo-shear]` | yes | — | enforced; cargo-udeps (nightly, compile-precise) is planned |
| Swift | SwiftLint 0.65.1 (`swiftlint lint --strict` on `clients/ios clients/macos`) | `.swiftlint.yml` | yes | — | enforced; three SourceKit rules are skipped on Linux |
| Kotlin | detekt 1.23.8 CLI on `android` | `detekt.yml`, `detekt-baseline.xml` | yes | — | new findings enforced; existing findings baselined; not wired into Gradle |
| Web / JS | — | none | — | — | not configured |

The quality checks are `tests/scripts/rust-check.sh quality`, run in the dev container by `make quality` (also `make lint`,
`make check`, `tests/scripts/check-quality.sh`) and directly on the runner by CI job `ci / core / quality`. Git hooks run only
the fast staged checks, see [code-quality.md](code-quality.md#git-hooks).

## Rust

Quality container (the gate):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets --locked --offline -- -D warnings
```

Cargo denies Clippy `allow_attributes` and `allow_attributes_without_reason` in workspace and
standalone crates. Handwritten Rust suppressions use `#[expect(..., reason = "...")]` so
`unfulfilled_lint_expectations` reports them once the underlying lint stops firing;
`-D warnings` makes that a quality failure. Generated code remains covered by narrowly
reasoned expectations at its include or derive boundary.

`-D warnings` turns the **default** clippy lint groups (`correctness`, `suspicious`, `style`,
`complexity`, `perf`) into errors. `pedantic`, `nursery` and `restriction` lints (for example
`arithmetic_side_effects`) are not generally enabled. A separate
[B8 check](code-quality.md#explicit-panic-budgets) force-enables `unwrap_used`,
`expect_used` and `panic` on non-test workspace targets and enforces their budgets.

`codeql.yml` runs a separate clippy step with `-W clippy::all -W clippy::pedantic -D warnings` and
the cargo-deny checks, all with `continue-on-error`: they are advisory and mark only their step
failed, while the gating clippy and cargo-deny runs are in `make quality`. A second clippy run
converts its JSON output to SARIF for code scanning. That job runs outside Docker.

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

The quality step runs `swiftlint lint --strict --quiet --no-cache clients/ios clients/macos` (the source tree is
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
- Write it right the first time: inside a type, order type properties, instance properties,
  initializers, type methods, then other methods (`type_contents_order`); write `Self` for the
  enclosing type in static members (`prefer_self_in_static_references`); sort imports by module
  name, `@testable` ones included (`sorted_imports`); pass the last closure argument as a
  trailing closure (`trailing_closure`); put each `case` body of a `switch` on its own line and
  leave one empty line between cases (`switch_case_on_newline`,
  `vertical_whitespace_between_cases`); keep a `///` doc comment directly above its declaration,
  with any `//` comment above it and a blank line between (`orphaned_doc_comment`).

`swiftlint --fix` rewrites only rules that support correction; review the diff.

## Kotlin (`detekt.yml`)

The quality step runs the detekt 1.23.8 CLI jar (sha256-checked, on Java 21 in the dev image):
`java -jar /usr/local/lib/detekt-cli.jar --config detekt.yml --baseline detekt-baseline.xml
--input clients/android --excludes '**/build/**' --build-upon-default-config`.
`detekt-baseline.xml` records 43 remaining findings; new findings fail the check.
`--build-upon-default-config` keeps every default setting `detekt.yml` does not override.
The Gradle build applies no detekt plugin, so `./gradlew detekt` does not exist.
The config sets `CyclomaticComplexMethod` threshold 10,
`LongMethod` 30, `LargeClass` 150, `LongParameterList` 5 (functions) / 7 (constructors),
`MaxLineLength` 120; most rule sets active, including the `Undocumented*` comment rules for public
declarations. `FunctionNaming` ignores `@Composable` functions, `MagicNumber` ignores property
declarations, and `FunctionMaxLength` skips the test sources. The `formatting` (ktlint) rule set
is not used.

The baseline suppresses only its recorded findings; it does not fix the code. Work to remove
those findings and then the baseline is tracked separately. When fixing a finding, remove its
entry from `detekt-baseline.xml` so the baseline reflects only remaining debt.
A baseline ID includes the full function signature (XML-escaped, e.g. `-&gt;`), so changing the
parameters of a baselined function needs its ID updated to the new signature.

Defaults that new Android code often hits: `TooManyFunctions` reports at 10 functions per class
and 11 per file (top-level extension functions in another file help), `StringLiteralDuplication`
counts repeated `testTag` strings, `BracesOnIfStatements` wants braces on a multi-line `if`, and
`UseIfInsteadOfWhen` rejects a two-branch `when`. `CommentOverPrivateFunction` rejects KDoc on a
private function (use `//` comments), and each `&&`, `||` and `when` branch adds to
`CyclomaticComplexMethod`, so move a branchy `when` into its own function.

Before checking Android changes, read the diff against these limits: every new
`values/strings.xml` entry has a `values-ru` counterpart (D1); no touched function exceeds 30
lines or 4 parameters; no line exceeds 120 characters; every new public declaration, including
each enum constant, has KDoc; a class stays under 10 functions. Then run `make lint-android`
(a few seconds): it runs `check-repository.py` (D1) and detekt, both to the end, and lists every
finding after `DETAILS:`; read that whole list, not only the `FAIL` line. Run `make quality`
once it passes.

## Suppressing a finding

Fix the code first. If a suppression is justified, scope it as narrowly as possible and state why:

```rust
#[expect(clippy::too_many_arguments, reason = "mirrors the C ABI signature")]
fn ffi_entry(/* … */) {}
```

```python
import optional_module  # noqa: F401  (re-exported for callers)
```

A suppression comment only silences the named lint at that spot. It does not satisfy other lints,
and it does not apply to other linters (an `#[expect]` for a default Clippy lint does not hide
unrelated findings in the `codeql.yml` pedantic run).

There are currently no Kotlin `@Suppress` annotations or JavaScript `eslint-disable` comments
in the clients. [Detekt 1.23.8](https://detekt.dev/docs/1.23.8/intro/) supports suppressing
findings but has no native unused-suppression check in its configured rules; its `Unused*`
rules check code, not annotations. The desktop JavaScript has no ESLint setup, so ESLint's
[`reportUnusedDisableDirectives`](https://eslint.org/docs/latest/use/configure/configuration-files#report-unused-disable-directives)
cannot be enabled without adding a linter. If ESLint is introduced, set this option to `error`.

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
