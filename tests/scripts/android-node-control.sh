#!/bin/bash
# Single-client, signal-driven lifecycle control for the emulator container's test node.
set -euo pipefail
mode=${1:?expected init, serve or command}
root=${2:?expected control directory}
shift 2

stop_node() {
    if [[ -n $node_pid ]]; then
        kill "$node_pid" 2>/dev/null || true
        wait "$node_pid" 2>/dev/null || true
        node_pid=''
    fi
}

serve() {
    local action
    node_pid=''
    exec {request_fd}<>"$root/request" {reply_fd}<>"$root/reply"
    trap stop_node EXIT
    trap 'exit 143' TERM INT
    "$@" {request_fd}>&- {reply_fd}>&- >> "$root/node.log" 2>&1 &
    node_pid=$!
    while IFS= read -r -u "$request_fd" action; do
        case "$action" in
            start)
                if [[ -z $node_pid ]] || ! kill -0 "$node_pid" 2>/dev/null; then
                    stop_node
                    "$@" {request_fd}>&- {reply_fd}>&- >> "$root/node.log" 2>&1 &
                    node_pid=$!
                fi
                # A spawn acknowledgement is not listener readiness.
                printf 'start spawned %s\n' "$node_pid" >&"$reply_fd"
                ;;
            stop|shutdown)
                stop_node
                printf '%s stopped\n' "$action" >&"$reply_fd"
                [[ $action != shutdown ]] || return 0
                ;;
            status)
                if [[ -n $node_pid ]] && kill -0 "$node_pid" 2>/dev/null; then
                    printf 'status running %s\n' "$node_pid" >&"$reply_fd"
                else
                    stop_node
                    printf 'status stopped\n' >&"$reply_fd"
                fi
                ;;
            *) printf 'error unknown-command\n' >&"$reply_fd" ;;
        esac
    done
}

command() {
    local action=${1:?expected lifecycle action} reply
    case "$action" in
        start|stop|status|shutdown) ;;
        *) echo "Unknown node action: $action" >&2; return 2 ;;
    esac
    # Callers bound this entire exchange, including FIFO opening, with timeout.
    printf '%s\n' "$action" > "$root/request"
    IFS= read -r reply < "$root/reply"
    [[ $reply == "$action "* ]] || {
        echo "Unexpected node acknowledgement: $reply" >&2
        return 1
    }
    printf '%s\n' "$reply"
}

case "$mode" in
    init) mkfifo "$root/request" "$root/reply" ;;
    serve) serve "${@:?expected node executable}" ;;
    command) command "$@" ;;
    *) echo "Unknown node control mode: $mode" >&2; exit 2 ;;
esac
