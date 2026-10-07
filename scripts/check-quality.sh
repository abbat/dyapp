#!/bin/bash
set -euo pipefail
case "${1:-}" in
    '') exec bash "$(dirname "${BASH_SOURCE[0]}")/docker-test.sh" quality ;;
    --coverage) exec bash "$(dirname "${BASH_SOURCE[0]}")/docker-test.sh" coverage ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
esac
