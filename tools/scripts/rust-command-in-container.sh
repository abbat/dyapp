#!/bin/bash
set -euo pipefail
bash tests/scripts/check-container-isolation.sh
rustc -Vv
cargo -V
exec "$@"
