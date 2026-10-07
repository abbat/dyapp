#!/bin/bash
set -euo pipefail
bash scripts/check-container-isolation.sh
python3 -B scripts/check-repository.py
actionlint
for script in scripts/*.sh; do bash -n "$script"; done
python3 -B -m unittest discover -s tests -p 'test_*.py' -v
python3 -m flake8 scripts tools tests
cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets --locked --offline -- -D warnings
cargo check --workspace --all-features --all-targets --locked --offline
cargo test --workspace --all-features --locked --offline -- --test-threads=1
