# Linting

What each linter checks, where it actually runs, and what is only a config file. CI wiring is
described in [ci-base.md](ci-base.md#quality--quality) and
[ci-matrix.md](ci-matrix.md#other-workflows); whether any check is required for merge is **not
established** ([required checks](ci-matrix.md#required-checks)).

## New languages and areas

A new language, client or file type gets its checks in the same change that adds its first file:

- a formatter in check mode;
- a lint with limits on function complexity, length and nesting;
- dead code: unused files, exports, declarations and resources;
- type checking where the language has it, in strict mode;
- the file size limit in `tests/scripts/check-repository.py`.

Each tool has a pinned version in the quality image and a row in the table below. A check that
cannot run yet is listed with a planned status and a tracking issue; it is not left out. Adding
checks later costs more: the backlog of violations has to be reformatted, budgeted or rewritten
first.

`make quick-check` runs Rust formatting, web lint/format and Swift lint/format in the existing
dev Docker container without compiling code. `make quality` remains the full gate, including
Clippy. Prepare the images after Dockerfile changes before using these commands.

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
| Docs with the code | `tests/scripts/docs-sync.py` ([rules](code-quality.md#docs-change-with-the-code)) | `RULES` in the script | yes | — | enforced: commits since the push or PR base |
| Spelling | typos on the source tree | `_typos.toml` | yes | — | enforced |
| TOML | `taplo fmt --check` | `.taplo.toml` | yes | — | enforced |
| Cargo dependencies | cargo-shear (unused dependencies) | `[package.metadata.cargo-shear]` | yes | — | enforced; cargo-udeps (nightly, compile-precise) is planned |
| Swift | SwiftLint 0.65.1 and strict swift-format from pinned Swift 6.4.0 | `.swiftlint.yml`, `.swift-format` | yes | — | enforced; three SourceKit rules are skipped on Linux |
| Kotlin | detekt 1.23.8 CLI on `android` | `detekt.yml`, `detekt-baseline.xml` | yes | — | enforced; the baseline is empty; not wired into Gradle |
| JavaScript, HTML, CSS | Biome 2.6.3 (`biome check` on desktop and Android prototype) | `biome.jsonc` | yes | — | enforced; lint and formatting |
| Markdown | rumdl 0.3.1 (`rumdl check .`) | `.rumdl.toml` | yes | — | enforced; `.beads/` excluded |
| Dockerfiles | hadolint 2.15.1 on `tools/docker`, `tests/docker` | `.hadolint.yaml` | yes | — | enforced |
| Protobuf | `buf lint` 1.75.0 on `proto` | `proto/buf.yaml` | yes | — | enforced; `STANDARD` rules |

The quality checks are `tests/scripts/rust-check.sh quality`, run in the dev container by `make quality` (also `make lint`,
`make check`, `tests/scripts/check-quality.sh`) and directly on the runner by CI job `ci / core / quality`. Git hooks run only
the fast staged checks, see [code-quality.md](code-quality.md#git-hooks).

## Rust

Native Linux CI runs Clippy on every target before building and testing the desktop crate.
`make lint-linux` runs that Clippy check alone in the Linux test container; it requires the
Linux images prepared with Clippy and rustfmt 1.99.0. The command may compile dependencies
on a cold cache. Tauri's generated capability schemas use a separate bind mount from
`build/linux-tauri-gen`; the source checkout stays read-only. `make fmt` checks formatting
without compilation. Native desktop manifests also deny `clippy::assert_is_empty`: test assertions
compare collections to explicit empty values, so failures show their contents.
Native manifests also deny the resolved pedantic rules for redundant method closures, implicit
cloning, clone assignment, ambiguous defaults, empty strings, ignored unit patterns, result
mapping, bool-to-int conversions, inline format arguments, literal readability, wildcard imports
and Rustdoc markup. The message view retains one exact expectation for mapping a timestamp
method: that type belongs to a transitive messaging dependency and is not directly importable.
Native ownership checks also deny `needless_pass_by_value`. Private status, error-detail and
video helpers borrow their inputs. Tauri commands, the permission callback and `Result::map_err`
classifiers retain owned contracts with declaration-level expectations and reasons; no module
suppression hides new helper findings. Place-picker IPC wrappers live in `clients/desktop/places.rs`
so these markers keep the main module below its file-size limit; IPC command names are unchanged.
Native manifests deny narrowing, wrapping, sign-loss and lossless `as` casts. UTC days and video
microseconds use checked conversion and reject unsupported clocks. Photo labels preserve hour
rounding for valid times; unsupported clocks or overflowing deadlines label the original expired,
without changing retained files or the core retention policy. Test fixture indices and QR sizes
also use explicit checked conversion. Window geometry's pure
math uses widened edges, saturating visible-area accumulation and clamped saved coordinates;
quarter visibility and title-bar requirements stay unchanged. Boundary tests execute the actual
geometry module with rustc in Docker, without linking Tauri.
Native shape/naming rules also deny excessive boolean fields, repeated struct/field names,
similar bindings, single-variant wildcards and allocating `format!`/collect loops. The two
serialized message/privacy DTOs retain checked expectations for independent flags; their IPC
fields stay unchanged. Private thumbnail frame naming is distinct from the `pixels` field.
A shared encoder produces identical lowercase hex without per-byte formatting allocations;
its test covers all byte values and leading zeros.
Linux denies the complete `pedantic` group; the three function-complexity rules retain the exact
legacy budgets described below. Large awaited futures use `Box::pin` at measured call boundaries,
including tests, preserving awaited results and cancellation while moving their storage to the heap.
Windows also denies `large_futures`; its full pedantic qualification remains planned pending native
Windows verification. All expectations are checked by `unfulfilled_lint_expectations`.
Cargo fmt does not traverse `include!`, so `tests/scripts/desktop-rust-format.py` explicitly
checks every Rust source under `clients/desktop`, including unreferenced files. It fails if no
sources are found. Both `make fmt` (also quality and quick-check) and `make fmt-write` run it
in the dev container with the pinned rustfmt and native crates' Rust 2021 edition.
A real formatter fixture covers included sources, unreferenced sources and empty discovery.

Quality container (the gate):

```bash
cargo fmt --all -- --check
cargo fmt --manifest-path clients/linux/Cargo.toml -- --check
cargo fmt --manifest-path clients/windows/Cargo.toml -- --check
python3 -B tests/scripts/check-rust-complexity.py --scope workspace
```

The root workspace, bootstrap/FFI crates and native desktop manifests deny `unreachable_pub`
and `unfulfilled_lint_expectations`. Existing internal module APIs keep their public spelling
and implementations, with declaration-level `expect(unreachable_pub, reason = ...)` markers.
This lint describes export visibility, not whether a function executes: retained markers do not
claim that used code is dead. New unclassified visibility findings and stale markers fail Rust
compilation. Real compiler fixtures test unmarked, retained, newly added and newly exported APIs.
Four desktop command modules also carry a module-level expectation for public wrapper reexports
emitted by Tauri: its macro does not propagate declaration attributes to those generated items.
These boundaries cover `chats/names.rs`, `clipboard.rs`, `link.rs` and `safety.rs`; handwritten
API declarations retain their individual markers.

Cargo denies Clippy `allow_attributes` and `allow_attributes_without_reason` in workspace and
standalone crates. Handwritten Rust suppressions use `#[expect(..., reason = "...")]` so
`unfulfilled_lint_expectations` reports them once the underlying lint stops firing;
`-D warnings` makes that a quality failure. Generated code remains covered by narrowly
reasoned expectations at its include or derive boundary.

`-D warnings` turns the **default** clippy lint groups (`correctness`, `suspicious`, `style`,
`complexity`, `perf`) into errors. Linux additionally denies `pedantic`; other crates enable
selected rules. `nursery` and `restriction` lints (for example `arithmetic_side_effects`) are not
generally enabled. A separate
[B8 check](code-quality.md#explicit-panic-budgets) force-enables `unwrap_used`,
`expect_used` and `panic` on non-test workspace targets and enforces their budgets.

The dev image and CI bootstrap provision [Hawk 0.1.17](https://github.com/astral-sh/hawk/tree/0.1.17)
with a pinned release checksum. Its compiler wrappers require the existing Rust 1.99.0
toolchain. `make quality` and `make lint-rust-api` reject unclassified `hawk::dead_public`
findings. The analysis includes all features, tests, doctests, the shipped daemon and FFI core;
missing coverage, malformed reports and compiler failures fail the check. The report and compiler
log stay in `build/unused-public.json` and `build/unused-public.stderr`, also archived by CI.

`hawk.toml` marks existing unused APIs with exact `expect` entries and reasons. No core code is
removed: video APIs remain as groundwork for the next version. Separate entries explain real
standalone desktop consumers and generated protobuf enum reflection helpers. An expectation
fails when its item becomes used, disappears or no longer matches. New unused APIs require
classification rather than increasing an aggregate budget or excluding a crate.

Hawk is experimental. The quality suite tests live cross-crate calls, dead functions/constants,
external exports and stale expectations on a two-crate fixture. The runner uses a 64 MiB main
stack and 128 MiB Rust worker stack, required by the measured workspace analysis. Strict
Rustdoc uses the same stack limits after a reproduced compiler stack failure. `make doc` and
quality both check workspace documentation with all features and warnings denied. These checks
compile code and are deliberately separate from the compilation-free `make quick-check`.

`codeql.yml` runs a separate clippy step with `-W clippy::all -W clippy::pedantic -D warnings` and
the cargo-deny checks, all with `continue-on-error`: they are advisory and mark only their step
failed, while the gating clippy and cargo-deny runs are in `make quality`. A second clippy run
converts its JSON output to SARIF for code scanning. That job runs outside Docker.

CodeQL also compiles Android Kotlin with `:app:compileDebugKotlin` and the iOS Swift app with
`xcodebuild`, then runs `security-and-quality` analysis and uploads SARIF for both languages.

Run locally: `make quality` (all checks) or `make fmt` (format check only; it does not rewrite files). To reformat, run `make fmt-write`; it uses the dev container with a writable source mount and covers the root, Linux, Windows and standalone function mapper manifests.

### Rust function limit tooling

`tools/rust-functions` is a pinned, standalone syn parser. Quality runs its tests and Clippy;
format checks include it. It maps current source spans to module/impl/function identities and
measures block nesting, including inline module and impl scopes. Anonymous closures are marked
separately so they cannot inherit a named function's cognitive-complexity budget. Parse failures
are errors. Workspace quality and `make lint-linux` enforce 15 cognitive complexity, 50 code
lines and 5 nesting levels with exact existing-function budgets. The tested
`tests/scripts/rust_complexity.py` comparison layer rejects incomplete reports, unknown
functions, stale source spans, anonymous cognitive-complexity findings and metrics above budgets.
It requires improvements to lower their recorded budgets, and checks both maximum nesting and
the number of excessively nested regions. The explicit runner
`tests/scripts/check-rust-complexity.py --scope workspace` (also `linux` or `windows`) retains metadata,
Clippy output, parser output and its verdict under `build/complexity/`. It preserves production
secret/crypto lint settings and rejects per-item cognitive threshold overrides, including those
in files without diagnostics. CI compares budgets against the push or PR base through
`COMPLEXITY_BASE`; local runs use `origin/main`. Allowances can only shrink. Detailed reports
remain in the local/container build directory; no extra report artifact is uploaded.
Windows measurements and native CI wiring remain planned. Run local checks through the existing
Docker containers; Git and parser dependencies are provisioned in the Linux image.

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

`make lint-swift` runs SwiftLint and strict swift-format checks in the dev container.
`make fmt-swift` applies the formatter to handwritten iOS and macOS application, unit-test
and UI-test sources; generated bindings are excluded by explicit input directories.
`.swift-format` selects four spaces and 100 columns. The formatter comes from the official
Swift 6.4.0 Jammy image, pinned by digest in the dev Dockerfile and shared runner script.
CI uses that same image through `tests/scripts/swift-format.sh`, without host Swift.
Swift-format owns brace and closure layout and collection commas; the conflicting SwiftLint
`opening_brace`, `closure_parameter_position` and `trailing_comma` rules are disabled.
SwiftLint retains complexity, length and safety checks. Formatter source-rewrite rules are
disabled so this check changes layout only.
Formatting does not compile Apple applications; their native CI tests remain required.

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
`detekt-baseline.xml` is empty: every finding fails the check.
`--build-upon-default-config` keeps every default setting `detekt.yml` does not override.
The Gradle build applies no detekt plugin, so `./gradlew detekt` does not exist.
Detekt parses Kotlin but does not compile it: after API refactors, native Android CI or a
Gradle compile is needed to catch stale call sites that `make lint-android`, `make test`
and `make quality` cannot detect.
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

## Web (`biome.jsonc`)

The quality step and `make lint-web` run `biome check --error-on-warnings` on
`clients/desktop` and `clients/android/prototype`: JavaScript, HTML and CSS, including smoke
scripts. `make fmt-web` formats all these files in the dev Docker container with an explicit
writable source mount. Checks retain read-only mounts. Formatting uses two spaces, single
JavaScript quotes and 100 columns. Assist actions remain disabled.
Cognitive complexity 15 and function length 50 code lines are blocking for every desktop UI
and prototype JavaScript file, including future files in those directories. The test-only video
frame renderer, video smoke, settings list/navigation smoke, Clear dialog/composer/identifier smoke
and removal/chat-redraw/media-cache smoke helpers also enforce these limits. All sticker, common,
dispatcher, guard, profile/peer photo, photo settings and chat photo send smoke helpers, plus
chat photo fixtures, use the same limits.
Future `*-guard*.js` helpers are included.
Blank lines and immediate closure wrappers used to scope classic scripts are excluded; nested
functions remain checked.
Extending this scope to the remaining smoke scripts is planned after their measured backlog is
refactored. The recommended rules apply except three,
each justified in the config:
`noLabelWithoutControl` and `useHeadingContent` (label and heading text comes from the i18n
catalogs at runtime) and `noDescendingSpecificity` (`style.css` groups rules by component).

`biome lint --write` applies the safe fixes; review the diff. A button outside a form still
needs `type="button"`, and a decorative SVG needs `aria-hidden="true"`.

## Markdown (`.rumdl.toml`)

The quality step runs `rumdl check .` on every Markdown file except `.beads/`, which `bd` writes.
`MD013` (line length) is off: prose wraps at about 100 columns by hand, and table rows and URLs
cannot. `MD033` allows only `<a id="…">` anchors. `rumdl fmt .` fixes most findings; review the
diff, since a fix to one table row realigns the whole table. A fence without a language takes
`text`.

## Dockerfiles (`.hadolint.yaml`)

The quality step runs hadolint on `tools/docker/Dockerfile.*` and `tests/docker/Dockerfile.*`. Two
rules are off: `DL3008` (apt version pins; the base images are pinned by digest, and Debian drops
superseded versions from its mirrors) and `DL3066` (`USER root` always resolves). A stage that
pipes in `RUN` sets `SHELL ["/bin/bash", "-o", "pipefail", "-c"]` rather than suppressing `DL4006`.
Suppress a finding with a comment line above the instruction and the reason above that:

```dockerfile
# The cd lasts only this step; the next one builds from the WORKDIR.
# hadolint ignore=DL3003
RUN cd clients/android && ./gradlew assembleDebug
```

hadolint does not report an ignore that no longer matches; the
[exception audit](practices.md#exception-audit-inventory) reviews them.

A comment does not change the build cache; editing a `RUN` line rebuilds every later layer.

## Protobuf (`proto/buf.yaml`)

The quality step runs `buf lint proto` with the `STANDARD` rules. Three are off:
`PACKAGE_DIRECTORY_MATCH` and `DIRECTORY_SAME_PACKAGE` (protox compiles the flat `proto/`
directory) and `PACKAGE_VERSION_SUFFIX` (protocol IDs and packages carry no version; messages evolve
only by new fields). A finding is suppressed with a `// buf:lint:ignore <RULE>` comment on the
element, under a comment that gives the reason. Renaming a field or an enum value changes the
generated Rust names but not the wire.

## Suppressing a finding

Fix the code first. If a suppression is justified, scope it as narrowly as possible and state why:

```rust
#[expect(clippy::too_many_arguments, reason = "mirrors the C ABI signature")]
fn ffi_entry(/* … */) {}
```

```python
import optional_module  # noqa: F401 -- re-exported for callers
```

A Python `# noqa` must name its codes and give a reason after ` -- `. flake8 has no
unused-noqa check, so `make quality` runs `tests/scripts/check-noqa.py tests tools`: it reruns
flake8 with `--disable-noqa` on the files that have one and fails on a bare `# noqa`, a missing
reason, or a suppressed code flake8 no longer reports on that line. Delete such a code or the
whole comment. There is no `# nosec`: bandit is not used.

A suppression comment only silences the named lint at that spot. It does not satisfy other lints,
and it does not apply to other linters (an `#[expect]` for a default Clippy lint does not hide
unrelated findings in the `codeql.yml` pedantic run).

There are currently no Kotlin `@Suppress` annotations in the clients.
[Detekt 1.23.8](https://detekt.dev/docs/1.23.8/intro/) supports suppressing findings but has no
native unused-suppression check in its configured rules; its `Unused*` rules check code, not
annotations. In the desktop web UI a `biome-ignore lint/<group>/<rule>: <reason>` comment (`//`
in JavaScript, `/* */` in CSS, `<!-- -->` in HTML) suppresses the next line; Biome reports a
suppression that no longer matches a finding, and `--error-on-warnings` fails the check on it.

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
