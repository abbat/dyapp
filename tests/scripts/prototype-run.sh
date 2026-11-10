#!/bin/bash
set -euo pipefail
printf 'prototype: starting Xvfb\n' >&2
exec xvfb-run -a -s '-screen 0 1280x800x24' python3 /app/prototype-shots.py "$@"
