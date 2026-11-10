#!/bin/bash
# Same pinned formatter in the dev image and on CI runners, with no host Swift dependency.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
case "${1:-lint}" in
    lint) args=(lint --strict); access=ro ;;
    format) args=(format --in-place); access=rw ;;
    *) echo 'Usage: swift-format.sh lint|format' >&2; exit 2 ;;
esac
if (($#)); then shift; fi
args+=(--configuration .swift-format --recursive)
if (($#)); then
    args+=("$@")
else
    args+=(clients/ios/DYApp clients/ios/Tests clients/ios/UITests)
fi
if [[ -f /.dockerenv ]]; then
    exec swift-format "${args[@]}"
fi
exec docker run --rm --network none --read-only --cap-drop ALL \
    --security-opt no-new-privileges --user "$(id -u):$(id -g)" \
    -v "$PWD:/workspace:$access" -w /workspace \
    swift@sha256:042765b8f0014cc3b43c7a00e55f6cd450e37b1b2ac4ff70171b0bfaa597e3bf \
    swift-format "${args[@]}"
