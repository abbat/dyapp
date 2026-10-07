#!/bin/bash
set -euo pipefail
bash scripts/check-container-isolation.sh
report_dir="/workspace/target/coverage.${COVERAGE_RUN_ID:?Missing run ID}"
mkdir -p "$report_dir"
export CARGO_LLVM_COV_TARGET_DIR="$report_dir/build"
# Exclude dependency sources and this specific generated scaffold, never lib.rs.
excluded='/registry/|/dyapp[.]uniffi[.]rs$'
cargo llvm-cov --workspace --all-features --locked --offline --jobs 4 \
    --json --summary-only --ignore-filename-regex "$excluded" \
    --output-path "$report_dir/coverage.json" -- --test-threads=1
cargo llvm-cov report --lcov --ignore-filename-regex "$excluded" \
    --output-path "$report_dir/coverage.lcov"
cargo llvm-cov report --ignore-filename-regex "$excluded" \
    > "$report_dir/coverage.txt"
test -s "$report_dir/coverage.txt"
python3 scripts/check_coverage.py "$report_dir/coverage.json"
echo "Coverage artifacts: $report_dir"
