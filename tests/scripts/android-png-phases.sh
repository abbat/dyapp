#!/bin/bash
# Sourced lifecycle: the runner supplies png_phase, stop_photo_host and node_action callbacks.
if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    echo "Source the PNG lifecycle helper from the Android runner" >&2
    exit 2
fi

run_png_phases() {
    if ! png_phase prepare; then
        stop_photo_host || true
        png_phase cleanup || true
        return 1
    fi
    if ! stop_photo_host; then
        png_phase cleanup || true
        return 1
    fi
    if ! node_action stop png-node; then
        # A failed FIFO exchange is uncertain: never issue another node command.
        png_phase cleanup || true
        return 1
    fi
    local failed=0
    png_phase reopenOffline || failed=1
    node_action start png-node || failed=1
    png_phase cleanup || failed=1
    return "$failed"
}
