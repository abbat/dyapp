#!/bin/bash
# Rust workspace checks. CI runs them directly on the runner; locally they run inside the dev
# image (docker/compose.dev.yml), whose environment keeps cargo offline.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
excluded='/registry/|/dyapp[.]uniffi[.]rs$' # dependency sources and the generated scaffold
# The cov-* steps share one instrumented build: compiled once by cov-build, reused by cov-test,
# network and cov-report (each step re-derives the same environment).
instrument() {
    # Once per process: show-env sets RUSTC_WRAPPER to cargo-llvm-cov itself, and a nested
    # show-env (step coverage runs three steps) recurses through that wrapper.
    [[ -n ${CARGO_LLVM_COV_SHOW_ENV:-} ]] && return
    export CARGO_TARGET_DIR=target/llvm-cov-target
    eval "$(cargo llvm-cov show-env --sh)"
}
# Practice B5: nextest retries failures (.config/nextest.toml); nextest skips doctests.
tests() {
    cargo nextest run --workspace --all-features --locked --no-tests fail
    cargo test --workspace --all-features --locked --doc
    python3 -B scripts/check-flaky.py --raw "${CARGO_TARGET_DIR:-target}/nextest/default/junit.xml"
}
check() {
    local name=$1
    shift
    python3 -B scripts/run-check.py --log-dir "${DYAPP_CHECK_LOG_DIR:-target/check-output}" \
        "$name" -- "$@"
}
step() {
    case "$1" in
        quality)
            check batches python3 -B scripts/check-batches.py --raw --report target/batches.json
            check repository python3 -B scripts/check-repository.py --raw
            check triggers python3 -B scripts/triggers.py --raw
            check docs-sync python3 -B scripts/docs-sync.py --raw
            check actionlint actionlint
            check zizmor zizmor --offline --no-progress .github
            # Secrets in the whole git history (CI checks out with fetch-depth 0).
            check gitleaks env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=safe.directory \
                GIT_CONFIG_VALUE_0="$PWD" gitleaks git --no-banner --redact .
            check typos typos
            check taplo taplo fmt --check
            check shellcheck shellcheck scripts/*.sh
            check python-tests python3 -B -m unittest discover -s tests -p 'test_*.py' -v
            check flake8 python3 -m flake8 scripts tools tests
            check swiftlint swiftlint lint --strict --quiet --no-cache ios macos
            check detekt java -jar /usr/local/lib/detekt-cli.jar --config detekt.yml --input android \
                --excludes '**/build/**' --build-upon-default-config
            check rustfmt cargo fmt --all -- --check
            check cargo-shear cargo shear
            # Lint only: the tests run in the coverage build (CI job core, `make test` locally).
            check clippy cargo clippy --workspace --all-features --all-targets --locked -- -D warnings -D clippy::todo
            check panics python3 -B scripts/check-panics.py --raw
            check rustdoc env RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --locked --no-deps
            ;;
        build) cargo build --release --workspace --locked ;;
        test) tests ;;
        cov-build)
            instrument
            cargo llvm-cov clean --workspace
            # Same unit graph as cov-test, so the tests reuse every artifact.
            cargo test --workspace --all-features --locked --no-run
            ;;
        cov-test)
            instrument
            tests
            ;;
        cov-report)
            instrument
            local report_dir="target/coverage${COVERAGE_RUN_ID:+.$COVERAGE_RUN_ID}"
            mkdir -p "$report_dir"
            cargo llvm-cov report --json --summary-only --ignore-filename-regex "$excluded" \
                --output-path "$report_dir/coverage.json"
            cargo llvm-cov report --lcov --ignore-filename-regex "$excluded" \
                --output-path "$report_dir/coverage.lcov"
            cargo llvm-cov report --ignore-filename-regex "$excluded" > "$report_dir/coverage.txt"
            test -s "$report_dir/coverage.txt"
            python3 scripts/check_coverage.py --raw "$report_dir/coverage.json" "$report_dir/coverage.lcov"
            echo "Coverage artifacts: $report_dir"
            ;;
        coverage) step cov-build; step cov-test; step cov-report ;;
        mutants)
            # Practice B2: every mutant in Rust code changed since MUTANTS_BASE must be caught.
            local base=${MUTANTS_BASE:-origin/main} src=target/mutants-src
            [[ $base =~ ^0+$ ]] && base=origin/main # a push that creates a branch
            rm -rf "$src" target/mutants-tmp
            mkdir -p "$src" target/mutants-tmp
            git -c safe.directory="$PWD" diff --no-ext-diff --no-color "$base" -- '*.rs' \
                > target/mutants.diff
            # Tracked files but .beads: ignored and beads files may be private to the host user.
            git -c safe.directory="$PWD" ls-files -z -- ':!.beads' |
                tar --null --ignore-failed-read -T - -cf - | tar -xf - -C "$src"
            TMPDIR=$PWD/target/mutants-tmp cargo mutants --dir "$src" --output target \
                --in-diff target/mutants.diff
            ;;
        fuzz)
            # Practice B4: each target (FUZZ_TARGETS, default all) for FUZZ_SECONDS on fuzz/corpus.
            # libFuzzer runs the corpus first, so a saved crash is a regression test. New inputs go
            # to target/fuzz-corpus, crashes to target/fuzz-artifacts.
            local seconds=${FUZZ_SECONDS:-60} target
            cd fuzz
            # The targets' node stores live here, off the small /tmp of the dev container.
            rm -rf ../target/fuzz-tmp
            mkdir -p ../target/fuzz-tmp
            export TMPDIR=$PWD/../target/fuzz-tmp
            cargo fuzz build -s none --target-dir ../target/fuzz
            for target in ${FUZZ_TARGETS:-$(cargo fuzz list)}; do
                mkdir -p "../target/fuzz-corpus/$target" "../target/fuzz-artifacts/$target"
                # The binary itself: `cargo fuzz run` writes into the read-only fuzz/artifacts.
                "../target/fuzz/x86_64-unknown-linux-gnu/release/$target" \
                    "../target/fuzz-corpus/$target" "corpus/$target" \
                    -max_total_time="$seconds" -rss_limit_mb=4096 \
                    -artifact_prefix="../target/fuzz-artifacts/$target/"
            done
            ;;
        network)
            # Two real dyappd peers on the loopback, test-peer as the client. cov-build already
            # built both (cargo test builds a package's binaries when it has integration tests); a
            # separate `cargo build -p` would select different features and recompile crates.
            instrument
            python3 scripts/network-test.py --raw --local "$CARGO_TARGET_DIR/debug"
            ;;
        memory)
            # Release binaries, as deployed: resident memory must level off under repeated load.
            cargo build --release -p dyapp-bootstrap --bins --locked
            python3 scripts/network-test.py --raw --plateau "${CARGO_TARGET_DIR:-target}/release"
            ;;
        security)
            if [[ -d /opt/advisory-dbs ]]; then
                # The dev image ships the advisory DB; --frozen also skips fetching it. The workspace
                # is read-only, and without --locked cargo metadata rewrites Cargo.lock.
                check advisory-cache bash -c \
                    'mkdir -p /tmp/ai && cp -rf /opt/advisory-dbs /tmp/ai/advisory-dbs'
                for database in /tmp/ai/advisory-dbs/advisory-db-*; do
                    check advisory-db git -C "$database" log -1 --format="advisory DB: %H %cI"
                done
                check cargo-deny cargo deny --frozen check advisories bans licenses sources
            else
                check cargo-deny cargo deny --locked check advisories bans licenses sources
            fi
            ;;
        *) echo "Unknown step: $1" >&2; exit 2 ;;
    esac
}
if [[ ${1:-} != --raw ]]; then
    stage=${1:?step required: quality|build|test|coverage|cov-build|cov-test|cov-report|mutants|fuzz|network|memory|security}
    case "$stage" in
        quality|security) exec bash "$0" --raw "$@" ;;
        build) policy=rust-build ;;
        test|cov-build|cov-test) policy=rust-test ;;
        coverage|cov-report) policy=rust-coverage ;;
        mutants) policy=rust-mutants ;;
        fuzz) policy=rust-fuzz ;;
        network|memory) policy=rust-network ;;
        *) policy=rust-build ;;
    esac
    check "$policy" bash "$0" --raw "$@"
    exit
fi
shift
step "${1:?step required}"
