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
| `deb`, `image` | Docker build/package stages; `image-test.sh` |
| `ui-test`, `ui-test-android`, `ui-test-android-emulator`, `ui-test-linux` | `ui-test.sh`; Docker/native test frontends |
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
and test output is captured by the owning Docker stage. The Android emulator
is a streaming service. None of these is a supported standalone host check;
in particular, package-test containers copy only their self-contained script.

## Standalone check frontends

Check scripts below are under `tests/scripts/`; shared runners and release scripts are under `tools/scripts/`. Default validation mode is concise;
explicit `--raw` retains direct output. Help is direct and read-only where offered.
Shell aggregate raw mode uses the first argument.

| Entry points | Role |
|---|---|
| `check-repository.py`, `check-batches.py`, `docs-sync.py`, `triggers.py` validation, `check-panics.py`, `check-dependency-age.py`, `check-advisory-ignores.py` | repository and policy validation |
| `check_coverage.py` | coverage ratchet; metrics remain in report files |
| `check-junit-results.py`, `check-apple-results.py`, `check-instrumentation-results.py` | reject incomplete, empty or failing test inventories |
| `check-container-isolation.sh`, `check-auditable.py` | self-contained isolation/ELF checks; fixtures bind their fix pointers to the map |
| `network-test.py` | finite network/memory acceptance; raw mode retains node diagnostics |
| `run-cargo-unit-binaries.py`, `desktop-smoke.py` | real binary inventory and desktop test execution |
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
`codeql.yml` wraps its finite Clippy/cargo-deny checks without changing its
advisory policy; its data-producing steps are listed above.
`release-sizes.yml` and `reproducible.yml` retain measurement JSON and raw logs.

Verification includes real Rust test-binary failure status 101, subprocess and
signal failures, missing/invalid inventories, cleanup precedence, invalid CLI
arguments, nested frames, large raw logs and non-UTF-8 terminal display.
SDK-free fixtures verify dispatch and file consumers; they do not stand in for
native builds. Existing native CI supplies platform proof. Changes to this
inventory require checking every Make target, script entry point and workflow
again, including data consumers. Output normalization does not establish that
an unrelated product, performance or release gate has passed.
