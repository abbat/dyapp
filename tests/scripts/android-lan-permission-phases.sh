#!/bin/bash
# Run only against the suite-owned emulator/install. Revocation happens between instrumentations.
set -euo pipefail
mode=${1:?legacy or modern required}
report_dir=${2:?report directory required}
node=${3:?owned LAN node address required}
public_node=${4:-}
serial=emulator-5554
permission=android.permission.ACCESS_LOCAL_NETWORK
test_class=com.dyapp.LocalNetworkPermissionPhaseTests
checker="$(dirname "${BASH_SOURCE[0]}")/check-instrumentation-results.py"
mkdir -p "$report_dir"

phase() {
    local name=$1
    local -a public_args=()
    if [[ -n $public_node ]]; then
        public_args=(-e localNetworkPublicNode "$public_node")
    fi
    python3 "$(dirname "$checker")/android_instrumentation.py" \
        --report "$report_dir/local-network-$name.txt" --deadline 300 -- \
        adb -s "$serial" shell am instrument -w -r \
        -e localNetworkPermissionPhase "$name" -e localNetworkNode "$node" \
        "${public_args[@]}" -e class "$test_class" com.dyapp.test/androidx.test.runner.AndroidJUnitRunner
    python3 "$checker" "$report_dir/local-network-$name.txt" 1
    if [[ $mode == modern ]]; then
        local receipt="$report_dir/local-network-$name-receipt.json"
        timeout 30 adb -s "$serial" exec-out run-as com.dyapp cat \
            no_backup/lan-permission-phase.json > "$receipt"
        python3 "$(dirname "$checker")/android_lan_receipt.py" "$receipt" "$name" "$node" "$public_node"
    fi
}

set_permission() {
    timeout 30 adb -s "$serial" shell pm "$1" com.dyapp "$permission"
}

foreground() (
    # Permission changes may terminate the app: establish prompt state before this process starts.
    trap 'set_permission grant' EXIT
    set_permission revoke
    timeout 30 adb -s "$serial" shell pm clear-permission-flags com.dyapp "$permission" user-set user-fixed
    python3 "$(dirname "$checker")/android_instrumentation.py" \
        --report "$report_dir/local-network-foreground.txt" --deadline 300 -- \
        adb -s "$serial" shell am instrument -w -r \
        -e localNetworkForeground true -e class com.dyapp.LocalNetworkDialogTests \
        com.dyapp.test/androidx.test.runner.AndroidJUnitRunner
    python3 "$checker" "$report_dir/local-network-foreground.txt" 1
)

case "$mode" in
    legacy) phase legacy ;;
    modern)
        if [[ -z $public_node || $public_node == "$node" ]]; then
            echo "Modern permission phases require a separate non-LAN node address" >&2
            exit 2
        fi
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
        foreground
        ;;
    *) echo "Unknown local-network permission mode: $mode" >&2; exit 2 ;;
esac
