"""Flips the last byte, the signature, of every record in a node's search
index (profiles-idx.db or a later generation of it), so a client must drop
them all as forged. Usage: forge-search-index.py <node storage dir>."""
from pathlib import Path
import sqlite3
import sys

indexes = sorted(Path(sys.argv[1]).glob("profiles-idx*.db"))
if len(indexes) != 1:
    raise SystemExit(f"expected one search index, found {indexes}")
with sqlite3.connect(indexes[0], timeout=30) as db:
    rows = db.execute("SELECT `key`, `record` FROM `record`").fetchall()
    if not rows:
        raise SystemExit("no indexed record to forge")
    for key, record in rows:
        db.execute("UPDATE `record` SET `record` = ? WHERE `key` = ?",
                   (record[:-1] + bytes([record[-1] ^ 1]), key))
print(f"forged {len(rows)} indexed records")
