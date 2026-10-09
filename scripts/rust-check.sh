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
step() {
    case "$1" in
        quality)
            python3 -B scripts/check-repository.py
            actionlint
            for script in scripts/*.sh; do bash -n "$script"; done
            python3 -B -m unittest discover -s tests -p 'test_*.py' -v
            python3 -m flake8 scripts tools tests
            cargo fmt --all -- --check
            # Lint only: the tests run in the coverage build (CI job core, `make test` locally).
            cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
            ;;
        build) cargo build --release --workspace --locked ;;
        test) cargo test --workspace --all-features --locked -- --test-threads=1 ;;
        cov-build)
            instrument
            cargo llvm-cov clean --workspace
            # Same unit graph as cov-test, so the tests reuse every artifact.
            cargo test --workspace --all-features --locked --no-run
            ;;
        cov-test)
            instrument
            cargo test --workspace --all-features --locked -- --test-threads=1
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
            python3 scripts/check_coverage.py "$report_dir/coverage.json"
            echo "Coverage artifacts: $report_dir"
            ;;
        coverage) step cov-build; step cov-test; step cov-report ;;
        network)
            # Two real dyappd peers on the loopback, test-peer as the client. cov-build already
            # built both (cargo test builds a package's binaries when it has integration tests); a
            # separate `cargo build -p` would select different features and recompile crates.
            instrument
            python3 scripts/network-test.py --local "$CARGO_TARGET_DIR/debug"
            ;;
        security)
            if [[ -d /opt/advisory-dbs ]]; then
                # The dev image ships the advisory DB; --frozen also skips fetching it. The workspace
                # is read-only, and without --locked cargo metadata rewrites Cargo.lock.
                mkdir -p /tmp/ai
                cp -rf /opt/advisory-dbs /tmp/ai/advisory-dbs
                for database in /tmp/ai/advisory-dbs/advisory-db-*; do
                    git -C "$database" log -1 --format="advisory DB: %H %cI"
                done
                cargo deny --frozen check advisories bans licenses
            else
                cargo deny --locked check advisories bans licenses
            fi
            ;;
        *) echo "Unknown step: $1" >&2; exit 2 ;;
    esac
}
step "${1:?step required: quality|build|test|coverage|cov-build|cov-test|cov-report|network|security}"
