#!/usr/bin/env python3
"""Check stopped-node SQLite stores on a disposable restored copy."""

import argparse
from contextlib import closing
import importlib.util
import os
from pathlib import Path
import shutil
import sqlite3
import tempfile
import time


SCRIPT = Path(__file__).with_name("verify-bootstrap-backup.py")
SPEC = importlib.util.spec_from_file_location("verify_bootstrap", SCRIPT)
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


def copy_verified_file(snapshot, scratch, record):
    source = VERIFY.open_beneath(snapshot, record["path"])
    target = scratch / record["path"]
    target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW
    destination = os.open(target, flags, 0o600)
    with os.fdopen(source, "rb") as input_file:
        with os.fdopen(destination, "wb") as output_file:
            shutil.copyfileobj(input_file, output_file, 1024 * 1024)
    VERIFY.check_file(scratch, record)


def check_database(path):
    deadline = time.monotonic() + 120
    with closing(sqlite3.connect(path)) as database:
        database.set_progress_handler(lambda: int(time.monotonic() > deadline),
                                      1000)
        result = database.execute("PRAGMA integrity_check(1)").fetchone()
        if result != ("ok",):
            raise ValueError(f"SQLite integrity check failed: {path.name}")


def probe(snapshot, scratch=None):
    if snapshot.is_symlink():
        raise ValueError("snapshot must not be a symlink")
    root = snapshot.resolve(strict=True)
    if not root.is_dir():
        raise ValueError("snapshot is not a directory")
    header, count, dirs, records, keys = VERIFY.parse_and_hash(root)
    VERIFY.check_inventory(root, count, dirs)
    if header["hot_journal"]:
        raise ValueError("hot rollback journal blocks automatic preflight")
    with tempfile.TemporaryDirectory(prefix="dyappd-restore-",
                                     dir=scratch) as temporary:
        scratch = Path(temporary)
        for record in records:
            copy_verified_file(root, scratch, record)
        databases = [record["path"] for record in records
                     if Path(record["path"]).name in VERIFY.BACKUP.DATABASES]
        for path in databases:
            check_database(scratch / path)
    return header, keys, len(databases)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("snapshot", type=Path)
    parser.add_argument("--scratch", type=Path,
                        help="directory for restored copies (default: TMPDIR)")
    args = parser.parse_args()
    try:
        header, _, count = probe(args.snapshot, args.scratch)
    except (OSError, sqlite3.Error, UnicodeError, ValueError) as error:
        parser.exit(1, f"SQLite backup probe failed: {error}\n")
    print(f"SQLite databases verified: {count}; peer ID: {header['peer_id']}")


if __name__ == "__main__":
    main()
