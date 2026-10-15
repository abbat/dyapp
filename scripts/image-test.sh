#!/bin/bash
# Runs dyappd:latest as an operator would: read-only root, no capabilities, one volume.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
if [[ ${1:-} == --raw ]]; then
    shift
elif [[ ${DYAPP_VERBOSE:-0} != 1 ]]; then
    exec python3 "$root/scripts/run-check.py" \
        --log-dir "${DYAPP_CHECK_LOG_DIR:-/tmp/ai/check-output}" \
        image-test -- bash "$root/scripts/image-test.sh" --raw "$@"
fi
if (( $# )); then echo "Unknown option: $1" >&2; exit 2; fi
name=dyappd-image-test-$$
cleanup() {
    local image_status=$? cleanup_status=0
    docker rm -f "$name" >/dev/null 2>&1 || true
    docker volume rm -f "$name" >/dev/null || cleanup_status=$?
    if (( image_status != 0 )); then exit "$image_status"; fi
    exit "$cleanup_status"
}
trap cleanup EXIT
# A low proof of work keeps keygen fast.
run() {
    docker run --rm --read-only --cap-drop ALL --security-opt no-new-privileges:true \
        --network none -v "$name:/var/lib/dyappd" -e DYAPPD__NETWORK__ID_POW_BITS=8 "$@"
}
if run dyappd:latest >/dev/null 2>&1; then exit 1; fi
run dyappd:latest keygen >/dev/null
run -d --name "$name" dyappd:latest >/dev/null
sleep 3
# Subcommands run in the container as the volume's owner.
docker exec "$name" dyappd deny add 203.0.113.9/24 image test >/dev/null
docker exec "$name" dyappd status | grep -q "^deny_entries 1$"
if docker exec "$name" sh -c true >/dev/null 2>&1; then exit 1; fi
docker logs "$name" 2>&1 | grep -q "listening" || { docker logs "$name"; exit 1; }
test "$(docker inspect -f '{{.State.Running}}' "$name")" = true
echo "dyappd image runs read-only as 65532 with one volume and no shell"
