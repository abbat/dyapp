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
dev Docker container without compiling code. Kotlin lint/format, Protobuf lint/format and Python lint checks run there too.
`make quality` remains the full gate, including Clippy. Prepare the images after Dockerfile changes before using these commands.

## Where each linter runs

| Language | Tool | Config | Quality container / CI `quality` | Other CI | Status |
|----------|------|--------|-----------------------------------|----------|--------|
| Rust | `cargo fmt`, `cargo clippy` | built-in defaults (no `rustfmt.toml`); Cargo `[lints]` for [unsafe](code-quality.md#unsafe-policy), docs and suppression policy | yes | `codeql.yml` (clippy, see below) | enforced |
| Python | flake8; Ruff format 0.17.2; mypy 2.4.0 | `.flake8`, `.ruff.toml`, `.mypy.ini` | yes, on `tools tests` | `python-lint.yml`: shared Python stage | enforced |
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
| Swift (iOS only) | SwiftLint 0.65.1 and strict swift-format from pinned Swift 6.4.0 | `.swiftlint.yml`, `.swift-format` | yes | — | enforced; three SourceKit rules are skipped on Linux |
| Kotlin | detekt and detekt-formatting 1.23.8 on Android | `detekt.yml`, `detekt-baseline.xml`, `.editorconfig` | yes | Gradle `detekt`, `check` and app `preBuild` | enforced; empty baseline |
| Desktop UI JavaScript types | TypeScript 7.0.2 (`checkJs`, strict, no emit) | `rust-check.sh web-check` | yes | — | enforced |
| JavaScript, HTML, CSS | Biome 2.6.3 (`biome check` on desktop and Android prototype) | `biome.jsonc` | yes | — | enforced; lint and formatting |
| Markdown | rumdl 0.3.1 (`rumdl check .`) | `.rumdl.toml` | yes | — | enforced; `.beads/` excluded |
| Dockerfiles | hadolint 2.15.1 on `tools/docker`, `tests/docker` | `.hadolint.yaml` | yes | — | enforced |
| Protobuf | `buf lint` and `buf format` 1.75.0 on `proto` | `proto/buf.yaml` | yes | — | enforced; `STANDARD` lint and canonical formatting |

The quality checks are `tests/scripts/rust-check.sh quality`, run in the dev container by `make quality` (also `make lint`,
`make check`, `tests/scripts/check-quality.sh`) and directly on the runner by CI job `ci / core / quality`. Git hooks run only
the fast staged checks, see [code-quality.md](code-quality.md#git-hooks).

## Rust

Native Linux CI runs Clippy on every target and feature before building and testing the desktop
crate, including the opt-in `ui-smoke` code. Ordinary packages omit that feature; their external
release smoke checks and hook-byte rejection are described in [UI testing](../testing/ui-testing.md).
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
Linux denies the complete `pedantic` group and has no function-complexity baseline exceptions.
Startup keeps its IPC registry in a private function pointer, separate from lifecycle setup.
Mailbox paging and profile refresh share per-pass state; notification focus and tray reads
remain on the detached delivery thread. Large awaited futures use `Box::pin` at measured call boundaries,
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
reasoned expectations at its include or derive boundary. The generated node-protocol module
expects `doc_markdown`: prost's fixed enum documentation and copied schema comments do not
follow Rustdoc code markup. Handwritten documentation keeps the lint enabled.

The root workspace and the bootstrap/FFI manifests deny `missing_errors_doc`,
`missing_panics_doc`, `must_use_candidate` and `doc_markdown`. CI and `make quality`
therefore reject missing error/panic contracts, ignored return-value contracts and
Rustdoc markup violations on their checked targets. The compilation-free
`make quick-check` continues to cover formatting and web/Swift static checks.
Client envelope preparation lives in `rust/client/src/prepared.rs`, with stable root reexports;
session restoration and read-only accessors live in `rust/client/src/session/state.rs`.
The same Cargo manifests deny `assert_is_empty`, `manual_assert_eq`,
`uninlined_format_args`, `unreadable_literal` and `wildcard_imports`: assertions
show compared values, formatting captures local names, long numeric literals use
digit separators, and imports name their dependencies explicitly.
Root workspace and bootstrap/FFI manifests also deny `large_futures`. The WebRTC
constructor and node runner pin their large child futures on the heap. Shared
CLI and test connection helpers keep swarms on the heap, and the protocol test
person keeps its state boxed while publishing; these boundaries also shrink the
futures of their callers. Results, awaits, deadlines and cancellation remain unchanged.
The manifests also deny `needless_pass_by_value` and `trivially_copy_pass_by_ref`.
Internal helpers borrow read-only descriptors and results; small connection IDs,
pixel formats and fixture tags pass by value. Declaration-level checked expectations
preserve UniFFI's owned argument lifting, the public inventory handler's owned request
contract and `map_err` adapters that receive ownership of their source error.
The same manifests deny the complete `pedantic` group; only the exact complexity budgets below
cover existing long, nested or cognitively complex functions. Independent serialized flags retain
checked expectations at their declarations, and async trait/public contracts keep their signatures.
Private cursor decoding distinguishes invalid data from an empty position with `Result<Option<_>, _>`.
Live Cargo fixtures copy the three manifests' lint tables: clean code and a justified expectation
pass, while an unmarked pedantic finding or stale expectation fails.
A shared identity hex encoder avoids per-byte formatting allocations; all byte values and leading
zeros are tested. Decimal safety fingerprints and persisted peer lists write into a single string.
Large test peer arrays live on the heap, and deterministic nonce fixtures use bounded searches.
The manifests also deny narrowing, wrapping, sign-loss, precision-loss and lossless `as` casts.
Variable numeric values use checked conversions; unsupported client clocks return `ClockRange`.
Millisecond wall clocks saturate at their integer maximum instead of wrapping. Retention seconds
use signed timestamp arithmetic directly from the configured unsigned 32-bit hour count.
Checked expectations document bounded image/header arithmetic and intentional fractional quota
rounding; test byte streams explicitly select their low byte when modulo-256 data is intended.

`-D warnings` turns the **default** clippy lint groups (`correctness`, `suspicious`, `style`,
`complexity`, `perf`) into errors. The root workspace, bootstrap, FFI and Linux also deny
`pedantic`; Windows currently enables selected rules. `nursery` and `restriction` lints (for example `arithmetic_side_effects`) are not
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

`codeql.yml` uses the pinned Rust toolchain and repository Clippy policy to collect JSON diagnostics
and upload SARIF for code scanning. It does not repeat an advisory pedantic invocation. Its
cargo-deny reports remain advisory; Clippy, exact complexity budgets and cargo-deny are enforced by
`make quality`. The code-scanning job runs outside Docker.

CodeQL also compiles Android Kotlin with `:app:compileDebugKotlin` and the iOS Swift app with
`xcodebuild`, then runs `security-and-quality` analysis and uploads SARIF for both languages.

Run locally: `make quality` (all checks) or `make fmt` (format check only; it does not rewrite files). To reformat, run `make fmt-write`; it uses the dev container with a writable source mount and covers the root, Linux, Windows and standalone function mapper manifests.

### Rust function limit tooling

`tools/rust-functions` is a pinned, standalone syn parser. Quality runs its tests and Clippy;
format checks include it. It maps current source spans to module/impl/function identities and
measures block nesting, including inline module and impl scopes. Anonymous closures are marked
separately so they cannot inherit a named function's cognitive-complexity budget. Test functions
are measured too; fixture and assertion helpers retain each scenario's original coverage and order.
Profile validation and its boundary tests have no complexity allowances; validation preserves
field, text, photo and device error precedence through separate private checks.
FFI network/photo work and MLS validation use private helpers; these keep generated native
interfaces unchanged and preserve mailbox refill and failed-Welcome key-package restoration.
Parse failures are errors. Workspace quality and `make lint-linux` enforce 15 cognitive complexity,
50 code lines and 5 nesting levels with exact existing-function budgets. The tested
`tests/scripts/rust_complexity.py` comparison layer rejects incomplete reports, unknown
functions, stale source spans, anonymous cognitive-complexity findings and metrics above budgets.
It requires improvements to lower their recorded budgets, and checks both maximum nesting and
the number of excessively nested regions. The explicit runner
`tests/scripts/check-rust-complexity.py --scope workspace` (also `linux` or `windows`) retains metadata,
Clippy output, parser output and its verdict under `build/complexity/`. It preserves production
secret/crypto lint settings and rejects per-item cognitive threshold overrides, including those
in files without diagnostics. CI compares budgets against the push or PR base through
`COMPLEXITY_BASE`; local runs, a new branch and a forced push whose old head is gone use
`origin/main`. Allowances can only shrink. Detailed reports
remain in the local/container build directory; no extra report artifact is uploaded.
Windows measurements and native CI wiring remain planned. Run local checks through the existing
Docker containers; Git and parser dependencies are provisioned in the Linux image.

## Script references

The repository check follows script references from commands, documentation and source files
into `tests/scripts/` and `tools/`. Python imports count as references; a script's own filename
does not. Disconnected cycles fail even when their scripts refer to each other.
Exact manual entry points may be declared with reasons in
`tests/scripts/repository_unused.py`; empty, missing and stale entries fail. The list is empty.

The repository check also rejects unused Apple catalogue keys. Literal references come from
shipped iOS/macOS Swift sources, excluding comments and tests. Dynamic profile labels and options
come from the actual shared Rust scale registry; publication reasons come from the native
publication contract. Removing a dynamic lookup removes those references. Unknown metadata
formats fail the check; no prefix wildcard keeps arbitrary new catalogue keys alive.

Desktop catalogue checks run in repository quality and the web stage, including quick-check.
They require matching English/Russian keys and reject unused entries. JavaScript references are
parsed with Acorn: finite values follow lexical bindings, calls, callbacks and object properties;
HTML contributes only translation attributes. Native dynamic values come from the actual DTO
unions, Rust scale registry and documented command error contracts. Unsupported dynamic lookups
fail; native domains only count while their corresponding lookup exists. Parser regression fixtures
cover unused keys, aliases, scope isolation, metadata and malformed inputs.

## Python

```bash
make lint-python                         # Docker, also part of quick-check
bash tests/scripts/rust-check.sh python-check  # provisioned native CI runner
```

`.flake8`: `max-line-length = 79`, empty `ignore`, standard excludes (`.git`, `__pycache__`,
`.venv`, `venv`, `build`, `dist`, …). McCabe complexity (`C901`) is limited to 10 for every
Python function, including test helpers; there are no complexity baselines or suppressions.
`make lint-python` checks Ruff formatting, flake8, strict mypy and exact `noqa` reasons in Docker; quality, CI and
`make quick-check` share that stage. The fast pre-commit hook checks staged Python syntax and
complexity 10 in one offline Docker batch, using index blobs under `build/pre-commit/`. Real-tool
fixtures check the 10/11 boundary, partial staging, syntax failures and suppression rejection.
Ruff 0.17.2 is provisioned from the same SHA-256-verified wheel in Docker and native CI.
`.ruff.toml` fixes 79 columns, double quotes and Python 3.11-compatible syntax, preserving
host-side script compatibility. `make fmt-python` formats all Python in `tests/` and `tools/`
through a writable Docker mount. The shared Python stage checks that same complete inventory
without changing source or compiling the application. flake8 remains the lint tool:
Ruff's lint rules do not reproduce every existing flake8 check. Only `E203` (slice spacing) and
`W503` (operator placement) are excluded because they conflict with Ruff formatting; complexity
and unused-code checks remain enabled. The formatter fixture rejects drift and accepts canonical
input; installer fixtures verify checksum, archive shape and bounded downloads.

Output wrappers share `DiagnosticPolicy`, `CheckPolicy` and the narrow `CheckRunner` protocol
in `tools/scripts/check_dispatch.py`. Reuse those contracts when adding diagnostic formats or
calling the dynamically loaded runner; a missing source loader fails before dispatch. Report
readers describe their consumed fields with `TypedDict`; these annotations preserve, rather than
replace, runtime validation of external reports. Unittest's no-value test/lifecycle contracts
use `-> None`; fixture helpers retain their produced result types. Scripts loading local Python
sources assert that their import spec and source loader exist before constructing the module.
Docker execution helpers use typed stream handles and Compose build/service contracts; artifact
size readers describe their measured fields separately from collected error lists. Functions
without a produced value declare `-> None`, including command and fixture helpers.
CLI test helpers declare subprocess result types and string argument sequences; binary process
output remains bytes until decoded. Fixture input types include deliberately malformed values
used by negative tests, including absent job conclusions and invalid coverage counts.
`tools/scripts/ci_contracts.py` shares API/CLI run, job, step and tracked-failure shapes
between CI helpers and fixtures. Trigger rules use a common name/glob/regex contract;
the docs checker consumes the dynamically loaded parser through a narrow protocol.
Scalar validators accept `object` until their existing runtime checks narrow the input.
CLI helpers distinguish filesystem paths, byte/text buffers and numeric timeouts; test source
loaders return `ModuleType`. GUI parser callbacks retain the standard HTML attribute types.
`tests/fixture_contracts.py` describes fixture records, command callbacks and backup options.
Fixture builders keep concrete output fields; negative controls use raw input views when they
intentionally corrupt a record. Type-only imports keep those schemas out of test startup.
`tools/scripts/quality_contracts.py` describes compiler spans, measured complexity, panic
reports, release artifacts, priority proposals and advisory records. Repository validators retain
concrete text-map, XML resource and size-policy inputs; raw budget validators narrow external
values before returning typed measurements. These schemas are imported only while type checking.
Network scenario helpers share CLI reply, peer, flood-count and RSS observation records in
`tools/scripts/network_contracts.py`; flood calls return status counts separately from command
reply fields. Search model and SQLite measurement helpers share numeric parameters, capacity
rows, benchmark cells and partial-run records in `tools/scripts/search_contracts.py`.
Backup helpers share validated manifest headers, file records and narrow loader interfaces in
`tools/scripts/backup_contracts.py`. Raw manifest values keep their existing validation before
becoming checked records; copied filesystem paths are distinct from serialized relative paths.
Release helpers share feed targets, candidate packages, inventory records and injected command/HTTP
interfaces in `tools/scripts/release_contracts.py`; validation, download limits and trust checks
remain in their existing owners.
Policy and reproducibility helpers share work-item, lockfile and build-report records in
`tools/scripts/policy_contracts.py`. Coverage readers distinguish changed line sets from measured
hit counts. Desktop inventory and benchmark observations use `tools/scripts/measurement_contracts.py`;
HTML boolean attributes retain their possible missing values, and process pipes are checked before
sampling. The deployment rehearsal consumes the shared network harness through a narrow protocol.
Native desktop probes and launchers share `tools/scripts/desktop_contracts.py`; passing identity
cases have required peer markers, while failure and marker-free cases may return none. GI callbacks
use their D-Bus, asynchronous-result and accessibility types. `tools/scripts/webkit2_contracts.pyi`
describes the consumed WebKit2 4.1 GTK3 API, including Cairo snapshots; the upstream PyGObject
stubs currently provide the WebKit6 GTK4 API. The local view is used only while type checking.
Windows-only icon/tray validators require mypy's `win32` platform; other tools and fixtures use
`linux`. Both groups together cover every Python source and local stub without import suppressions.
`check-python-types.py` enforces `.mypy.ini` and target Python 3.11 across the complete source and
local-stub inventory. It runs both platform groups even after a failure and writes caches only to
`build/python-types/`. `MYPYPATH` is cleared so local search-path overrides cannot change the gate.
Docker and native CI install the same mypy 2.4.0 and library/stub versions from the SHA-256 lock in
`tools/python-types/requirements.json`; supported execution ABIs are CPython 3.11, 3.12 and 3.13 on
Linux x86-64. The stdlib-only installer verifies all downloads and archive paths before writing
the isolated library directory, preserves the GI license, and selects GTK3/Gdk3/Soup3 wrappers
without executing upstream build code or installing runtime GI libraries. Pillow supplies its own
types; PyGObject and PyYAML use pinned stub packages.

The default library directory is `/opt/dyapp-python-types`; `DYAPP_PYTHON_TYPES` selects an alternate
managed location, as in standalone Python CI. The gate checks the installed ABI and lock digest;
a stale installation fails with `make prepare` as the repair command. Fixtures exercise checksum,
archive traversal, size, API-version and stale-lock failures, and prove that real Linux and Windows
type regressions both fail the same gate.

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
The iOS XcodeGen project enables Swift warnings as errors and explicit complete strict
concurrency checking for the app and both test targets. Native iOS CI validates these
compiler settings and the generated bindings.

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

The quality step and `make lint-android` run `tests/scripts/kotlin-format.sh lint`:
the SHA256-pinned Maven detekt CLI and detekt-formatting 1.23.8 jars use
`detekt.yml`, `detekt-baseline.xml` and every owned Kotlin source under
`clients/android`, excluding build output. `make quick-check` runs the same stage.
`detekt-baseline.xml` is empty: every finding fails the check.
`--build-upon-default-config` keeps every default setting `detekt.yml` does not override.
`make fmt-kotlin` applies the supported corrections in Docker, then requires a clean
check. It allows up to three correction passes because wrapping can expose indentation
changes; remaining findings require manual fixes. Formatting uses four spaces and 120
columns, matching EditorConfig and the existing detekt length rule.

From `clients/android`, `./gradlew detekt` runs the same pinned CLI, plugin and rules.
Root and app `check`, plus app `preBuild`, depend on that task. The dependency
verification metadata pins both jars and their Maven metadata. The task parses sources
without generating Rust bindings or compiling the Android app.
All Android Kotlin compilations treat warnings as errors through the built-in Kotlin
compiler options. Android Lint also treats warnings as errors, explicitly enables
`UnusedResources` and `UnusedIds` as errors, and checks test sources without a baseline.
The Android build/test workflow runs `:app:lintDebug`; the release image runs that task
and `:app:lintRelease`. The local Android build/test workflow uses the same gates.
Full text diagnostics and XML reports retain every finding; Android CI uploads
`build/android-build/android/app/reports/lint-results-*` with its check evidence.
The compilation-free `make lint-android` and `make quick-check` run detekt; SDK-based
Android Lint runs with the native Android builds. Profile metadata labels use explicit `ProfileLabels.kt` resource references. Update the
mapping and both resource locales when a scale or choice changes; native tests compare the
complete resource set with the Rust metadata. Kotlin binding generation uses
`clients/android/uniffi.toml` for the Android cleaner and its API 34 guard/JNA fallback.
Gradle 9.9.1 is pinned with the official distribution SHA256. Whole-project unused
non-private Kotlin declaration analysis remains planned.

Detekt parses Kotlin but does not compile it: after API refactors, native Android CI or a
Gradle compile is needed to catch stale call sites that `make lint-android`, `make test`
and `make quality` cannot detect.
The config sets `CyclomaticComplexMethod` threshold 10,
`LongMethod` 30, `LargeClass` 150, `LongParameterList` 5 (functions) / 7 (constructors),
`MaxLineLength` 120; most rule sets active, including the `Undocumented*` comment rules for public
declarations. `FunctionNaming` ignores `@Composable` functions, `MagicNumber` ignores property
declarations, and `FunctionMaxLength` skips the test sources. The `formatting` rule set
adds ktlint formatting checks. Its unused-import correction is disabled because the
existing `style/UnusedImports` rule owns that check; formatting does not remove imports.

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
`clients/desktop`, `clients/android/prototype` and `tools/web-lint`: JavaScript, HTML and CSS,
including smoke scripts and the analysis tools. `make fmt-web` formats all these files in the dev Docker container with an explicit
writable source mount. Checks retain read-only mounts. Formatting uses two spaces, single
JavaScript quotes and 100 columns. Assist actions remain disabled.
Cognitive complexity 15 and function length 50 code lines are blocking for every JavaScript file
under `clients/desktop`, including all production, prototype, video and native smoke scripts.
The directory-wide glob also covers future helpers. Blank lines and immediate closure wrappers
used to scope classic scripts are excluded; nested functions remain checked. All desktop scripts
also fit the 1000-line file limit without legacy size allowances. Shipped UI modules additionally
reject undeclared variables, so cross-file dependencies require imports instead of page lexical
globals. The module-entry regression test rejects classic and inline scripts in the shipped page.
Desktop DOM lookup boundaries carry JSDoc types and validate actual element constructors rather
than casting controls to broad element contracts. Native DOM smoke checks required/optional
lookups, empty lists and rejection of missing or wrong-kind elements.
Shared catalogue, identifier, peer-label, formatting, profile-control and sticker-frame helpers
carry JSDoc contracts. Identity, profile and chat DTOs mirror the native serialized fields,
including nullable values and discriminated states. `ipc.js` describes all 108 native commands
used by the shipped UI: names, argument tuples and result DTOs, including binary photo/sticker
responses, paginated sticker reads and explicit cache write/clear commands. Unknown command names
fail type checking.
Literal command names retain argument checking instead of widening to a generic string when
arguments are invalid.
Controller helpers include JSDoc for DTO inputs, callbacks, DOM elements and optional arguments.
Open-profile views, note/favorite actions, dialog targets and timers describe their nullable state;
controllers explicitly expose typed promises for asynchronous operation completion.
Message rows, photo decoders/encoders, search results and sticker helpers use native DTO input types;
photo file preparation requires `File`, and sticker labels require only emoji and pack name.
Photo decoding converts byte arrays to binary data, rejects unavailable canvas contexts and always
closes decoded bitmaps. Native foundation smoke checks the three byte representations, canvas
failure cleanup and thumbnail ownership.
Sticker action kinds reuse the native command's finite union. Sticker image/request targets,
picker/manager openings and cache changes have explicit state contracts, including nullable and
not-yet-loaded fields. Active-opening checks narrow the current manager/picker state.
Picker, manager and cache reads require a ready identity before invoking native commands.
Picker cursors are initialized explicitly; focus restoration validates HTML elements and disabled state.
Validated failed pack statuses retain a non-null failure code in their discriminated contract.
Settings collections accept generic item callbacks and retain typed deferred row builders: search
and pagination preserve each source index while only the visible page creates DOM rows. Saved
profile actions carry typed list/read context and button ownership. Chat sends and camera/profile
helpers declare their callback and DTO contracts; finite network status codes type variadic summaries.
Native mail/revocation event routing validates payload types. Privacy and photo preferences validate
finite select/radio values before invoking their fully typed native commands.
Chat list-row rendering is a separate module so controller typing stays within the file-size budget.
`web-check` also runs TypeScript 7.0.2 with `--allowJs --checkJs --noEmit --strict` on every
shipped `clients/desktop/ui/*.js` module. This gate runs in CI quality, `make quality`,
`make lint-web` and `make quick-check`; it needs no compilation or generated JavaScript.
Docker preparation and native CI setup install the same SHA-512-verified Linux x64 native
compiler with its standard libraries, without Node or npm at check time.
`contact-qr.js` owns QR pixels and camera frames; `profile-public.js` owns labelled public facts.
Profile scale names are checked against the native fourteen-field set, and draft serialization
requires all nine non-defaulted scales before sending a save command.

The same web stage runs Knip 6.44.2 for unused UI files, exports and dependencies, and an AST-based
CSS selector check using Acorn 8.15.0, CSS Tree 3.2.1 and parse5 8.0.0. Preparation locks all npm
transitives and uses pinned Node 24.22.2; checks run offline with no installs. The product has no
npm dependency or bundling step. The checker copies every UI JavaScript file into a fresh analysis
workspace under `build/`, derives application entries from HTML, and uses the actual Rust smoke
script inventory. It rewrites only static smoke import URLs for analysis, preserving test consumers.
Missing/empty inventories, unsupported imports and parse errors fail rather than skipping work.
CSS class and ID names are compared with HTML attributes and JavaScript string/template tokens in
the shipped UI and shared prototype. Exact runtime-built names need a reason in
`tools/web-lint/runtime-selectors.json`; stale allowances fail. This detects unreferenced names,
not whether a particular element-selector combination can match at runtime.
The web stage also runs `tools/web-lint/check-css.mjs` and its regression fixtures.
Static desktop/prototype styles belong in CSS, including SVG positioning. Nonzero
literal gaps and margins fail L2; `--space-*` definitions must lie on the 4 px grid.
Existing non-token spacing is recorded in `tools/web-lint/css-budget.json` with a
reason; new findings and stale allowances fail. Its spacing budget is intersected
with all committed versions, so removed allowances cannot return; full Git history
is required. Literal pixel definitions and zero are supported; computed spacing
definitions need checker support before they can be verified. Repeated literal colour and font
values require shared tokens when they have the same purpose. Different purposes
may retain separate values: record the reason and exact declarations under
`distinctSemantics` in that file. The check requires semantic review rather than
assuming equal values mean equal purposes; a new occurrence invalidates that record.
One-off colour/font literals, `currentColor` and `transparent` remain allowed.

The recommended rules apply except three, each justified in the config:
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

`make lint-proto` runs `buf lint proto` and `buf format proto --diff --exit-code`.
Both checks also run in quality, CI and `make quick-check`; `make fmt-proto` writes canonical
formatting in Docker. The root `.editorconfig` supplies UTF-8, LF, final newlines and the
language-specific indentation used by the formatters, including tabs for Make recipes.
A real formatter fixture verifies that drifting input fails and canonical input passes.

Protobuf lint uses the `STANDARD` rules. Three are off:
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
unrelated Clippy findings in the quality run).

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

- Full Windows-native `pedantic` qualification remains planned; its selected rules are enforced.
- Required status checks are not established.
