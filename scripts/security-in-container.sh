#!/bin/bash
set -euo pipefail
mkdir -p /tmp/ai
cp -rf /opt/advisory-dbs /tmp/ai/advisory-dbs
for database in /tmp/ai/advisory-dbs/advisory-db-*; do
    git -C "$database" log -1 --format="advisory DB: %H %cI"
done
exec cargo deny check --disable-fetch advisories bans licenses
