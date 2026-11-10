# Check output inventory

Finite project checks use the [shared contract](code-quality.md#check-output):
one `OK <id>` on success; `FAIL`, an owning `FIX` anchor and selected `DETAILS`
on failure, with the original exit status and a full raw-log path. Unknown
diagnostics still fail and retain their raw evidence. Nested mapped failures
produce one frame. Signals and mandatory cleanup failures remain failures.

The authoritative ID, parser and documentation-anchor map is
`tools/scripts/check-output.json`; repository validation checks it. The tables below
identify entry points and data consumers without maintaining a second map.

## Make and shell entry points

Make suppresses recipe echo; `make -n` shows the command plan. Composite targets
may run several finite stages, each with a result.

| Entry points | Quiet owner |
|---|---|
| `test`, `build`, `release`, `test-all`, `coverage`, `coverage-check`, `security`, `audit`, `deny` | `docker-test.sh`, Docker capture, mapped Rust stages |
| `test-integration` | mapped Cargo integration, network and memory stages |
| `quality`, `lint`, `check`, `pre-commit` | `check-quality.sh`; tool-level mapped checks |
| `fmt`, `msrv`, `doc` | explicit mapped Cargo commands inside Docker capture |
| `lint-web`, `quick-check`, `lint-python`, `lint-proto`, `lint-swift`, `lint-android`, `lint-linux`, `lint-rust-api` | mapped language/API checks and Docker capture; quick-check also runs formatting and SwiftLint |
| `deb`, `image` | Docker build/package stages; `image-test.sh` |
| `ui-test`, `ui-test-android`, `ui-test-android-emulator`, `ui-test-linux`, `ui-prototype`, `ui-prototype-android` | `ui-test.sh`; Docker/native test frontends |
| `mutants`, `fuzz`, `rehearsal` | mapped Rust stages inside Docker capture |
| `fmt-write`, `fmt-web`, `fmt-python`, `fmt-proto`, `fmt-kotlin`, `fmt-swift` | explicit formatting writers inside Docker capture |
| `pre-push` | `test-all` |
| `prepare`, `install` | Docker build capture; explicit preparation, subject to install permission |
| `help`, `ui-test-shell` | help or interactive mode, not a finite quiet check |

`docker-local.py` owns Docker build/run/configuration failures and stop cleanup.
`rust-check.sh` owns Rust build/test/coverage/network/memory stages and delegates
quality/security to their mapped tools. `linux-test.sh`, `build-apple-core.sh`,
`apple-test-in-ci.sh`, `apple-test-in-ci.py` and `build-android-release.sh` use mapped frontends.
Apple CI restrictions and Android offline dependency requirements still apply.

`android-test-in-container.sh`, `rust-command-in-container.sh`, `build-deb.sh`,
`deb-test.sh` and `tarball-test.sh` are internal stage bodies. Their raw compiler, package-manager
and test output is captured by the owning Docker stage. `client-deb-test.sh --session` also runs
its accessibility unit fixture inside that enclosing capture; the production UI driver has its
own mapped frontend. The production Windows composite action captures its PowerShell UI check
with the same production-smoke contract. The Android emulator
is a streaming service. None of these is a supported standalone host check;
in particular, package-test containers copy only their self-contained script.

## Standalone check frontends

Check scripts below are under `tests/scripts/`; shared runners and release scripts are under `tools/scripts/`. Default validation mode is concise;
explicit `--raw` retains direct output. Help is direct and read-only where offered.
Shell aggregate raw mode uses the first argument.

| Entry points | Role |
|---|---|
| `check-repository.py`, `check-batches.py`, `docs-sync.py`, `triggers.py` validation, `check-panics.py`, `check-unused-public.py`, `check-dependency-age.py`, `check-advisory-ignores.py` | repository and policy validation |
| `check_coverage.py` | coverage ratchet; metrics remain in report files |
| `check-junit-results.py`, `check-apple-results.py`, `check-instrumentation-results.py` | reject incomplete, empty or failing test inventories |
| `check-container-isolation.sh`, `check-auditable.py`, `check-hardening.py` | self-contained isolation/ELF checks; fixtures bind their fix pointers to the map |
| `network-test.py` | finite network/memory acceptance; raw mode retains node diagnostics |
| `run-cargo-unit-binaries.py`, `desktop-smoke.py`, `production-desktop-smoke.py` | real binary inventory and instrumented/production desktop execution |
| `check-no-ui-smoke.py` | reject empty/missing production artifacts and injected test hooks |
| `check-artifact-size.py`, `report-artifact-size.py`, `check-reproducible.py` | release validation/measurement; JSON files retain their schemas |
| `check-android-release.py`, `package-apple-release.py`, `apple-release-in-ci.py` | package/producer validation |
| `export-coverage.py`, `export-ui-results.py`, `export-ui-reports.py` | artifact copies; successful payloads are files, not stdout |
| `check-ci-results.py` | required-job results; missing, cancelled and skipped jobs fail |
| `search-model.py --self-check` | finite model assertions; default capacity report is separate data mode |

`run-check.py` is the shared runner. `check_dispatch.py`, `capture.py`,
`memory_report.py` and `flow_metrics.py` are libraries, not additional public
check commands. Existing import users remain quiet.

## Data and operator exceptions

These are existing consumers, not exemptions for an otherwise quiet test.

| Output producer | Actual consumer |
|---|---|
| `triggers.py` diff arguments | quorum gate reads trigger JSON |
| `quorum-check.py` JSON/report | local exact-commit delivery gate and quorum evidence |
| `priority-proposal.py` JSON/report/check | local proposal fingerprint and human approval protocol |
| `flow-report.py` JSON/report | local weekly flow report and snapshot comparison |
| `ci-report-import.py` | local operator action updates beads; never invoked by CI |
| `search-model.py` default / `--bench` | human capacity tables in [search capacity](../architecture/search-capacity.md) |
| `search-bench.py` | measurement JSON read by that model; explicit benchmark progress is diagnostic |
| `clippy-to-sarif.py` | SARIF file from redirected stdout |
| Clippy `--message-format=json > clippy.json` | SARIF conversion; stdout must remain Cargo JSON |
| `cargo sbom > sbom.json` | SPDX artifact upload |
| CodeQL unsafe-location listing | advisory source-location inventory in the job log, not a pass/fail gate |
| Coverage raw report mode | JSON/LCOV/text metrics and the coverage validator |
| Help, `--raw`, `DYAPP_VERBOSE=1`, interactive shells | explicit command plan, direct diagnostics or interaction |

`ci-setup.sh`, checkout/cache/toolchain actions, XcodeGen setup and third-party
CodeQL actions are preparation or externally owned actions. They retain their
tool metadata; project check wrappers do not rewrite action protocols or install
tools. User permission for preparation remains a separate requirement.

## CI ownership and evidence

`ci-base.yml` uses mapped Rust/quality/security stages. `ci.yml` uses mapped
Docker, Linux, Apple, Windows and required-summary checks. Native logs are
archived alongside their existing JSON/XML/screenshots. `python-lint.yml` and
the audit job in `security.yml` use mapped checks and archive full logs.
`codeql.yml` captures Kotlin/Gradle and Swift/Xcode compilation and Clippy stderr, in addition
to cargo-deny, without changing tracing or advisory policy. Compiler stdout for SARIF stays in
`clippy.json`; the original JSON and captured logs are archived. Kotlin and Swift retain their
own check-output artifacts. Scheduled advisory-date validation uses the concise default and
archives its raw evidence. Workflow steps must not request `--raw` directly; nested stage bodies
own that internal flag. Data-producing steps are listed above.
`release-sizes.yml` and `reproducible.yml` retain measurement JSON and raw logs.

Verification includes real Rust test-binary failure status 101, subprocess and
signal failures, missing/invalid inventories, cleanup precedence, invalid CLI
arguments, nested frames, large raw logs and non-UTF-8 terminal display.
SDK-free fixtures execute the actual CodeQL workflow build commands with noisy fixture tools:
both streams are captured, exit status is preserved, and Clippy JSON remains byte-for-byte data.
Guards check every workflow for direct raw-mode bypasses and every declared Python frontend
for a mapped diagnostic policy. These fixtures verify dispatch and
file consumers; they do not stand in for
native builds. Existing native CI supplies platform proof. Changes to this
inventory require checking every Make target, script entry point and workflow
again, including data consumers. Output normalization does not establish that
an unrelated product, performance or release gate has passed.
