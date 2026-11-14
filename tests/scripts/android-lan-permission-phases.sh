#!/bin/bash
# Run only against the suite-owned emulator/install. Revocation happens between instrumentations.
set -euo pipefail
mode=${1:?legacy or modern required}
report_dir=${2:?report directory required}
node=${3:?owned LAN node address required}
serial=emulator-5554
permission=android.permission.ACCESS_LOCAL_NETWORK
test_class=com.dyapp.LocalNetworkPermissionPhaseTests
checker="$(dirname "${BASH_SOURCE[0]}")/check-instrumentation-results.py"
mkdir -p "$report_dir"

phase() {
    local name=$1
    timeout 300 adb -s "$serial" shell am instrument -w \
        -e localNetworkPermissionPhase "$name" -e localNetworkNode "$node" \
        -e class "$test_class" com.dyapp.test/androidx.test.runner.AndroidJUnitRunner \
        > "$report_dir/local-network-$name.txt"
    python3 "$checker" "$report_dir/local-network-$name.txt" 1
}

set_permission() {
    timeout 30 adb -s "$serial" shell pm "$1" com.dyapp "$permission"
}

case "$mode" in
    legacy) phase legacy ;;
    modern)
        # Establish and restore a healthy fixture around the refused connection.
        set_permission grant
        phase granted
        set_permission revoke
        phase denied
        set_permission grant
        phase regranted
        set_permission revoke
        phase revoked
        set_permission grant
        phase cleanup
        ;;
    *) echo "Unknown local-network permission mode: $mode" >&2; exit 2 ;;
esac
