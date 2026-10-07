#!/bin/bash
set -euo pipefail
bash scripts/check-container-isolation.sh
rustc -Vv
cargo -V
exec "$@"
