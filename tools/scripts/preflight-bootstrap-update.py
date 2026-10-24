#!/usr/bin/env python3
"""Snapshot and prove a stopped node's storage before a package update."""

import argparse
from contextlib import contextmanager
import fcntl
import importlib.util
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile


SCRIPT = Path(__file__).with_name("probe-bootstrap-backup.py")
SPEC = importlib.util.spec_from_file_location("probe_bootstrap_backup", SCRIPT)
PROBE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROBE)
BACKUP = PROBE.SQLITE.VERIFY.BACKUP
MARGIN = 64 * 1024 * 1024


def available(path):
    return shutil.disk_usage(path).free


@contextmanager
def stopped(source):
    """Hold `dyappd.lock` so dyappd cannot start until the block ends."""
    path = source / "dyappd.lock"
    fd = os.open(path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    try:
        owner = source.stat()
        if os.fstat(fd).st_uid != owner.st_uid:
            # A root installer must not leave a lock dyappd cannot open.
            os.fchown(fd, owner.st_uid, owner.st_gid)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError(f"node still running: {path} is held") from None
        yield
    finally:
        os.close(fd)


def check_space(source, destination, key_mode, recovery_key, scratch,
                free=available):
    files, _ = BACKUP.paths(source, key_mode)
    sizes = {name: item.stat().st_size for name, item in files}
    total = sum(sizes.values())
    if recovery_key is not None:
        total += recovery_key.stat().st_size
    # An in-place migration may rebuild the largest table beside the old one.
    migration = max(sum(sizes.get(f"storage/{name}", 0)
                        for name in (db, f"{db}-wal"))
                    for db in BACKUP.DATABASES)
    need = [(destination.parent, total + MARGIN), (scratch, total),
            (source, migration)]
    # Paths on one filesystem share its free space.
    totals = {}
    for path, size in need:
        first, sum_ = totals.get(path.stat().st_dev, (path, 0))
        totals[path.stat().st_dev] = (first, sum_ + size)
    for path, size in totals.values():
        if free(path) < size:
            raise ValueError(f"not enough free space on {path}: "
                             f"need {size} bytes")


def preflight(source, destination, key_mode, recovery_key, dyappd,
              id_pow_bits, age_identity=None, age_binary=None, scratch=None,
              free=available):
    """Run inside `stopped(source)`; leaves live storage untouched."""
    scratch = Path(scratch or tempfile.gettempdir())
    check_space(source, destination, key_mode, recovery_key, scratch, free)
    BACKUP.snapshot(source, destination, key_mode, recovery_key)
    return PROBE.probe(destination, dyappd, id_pow_bits, age_identity,
                       age_binary, scratch)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="dyappd storage.dir")
    parser.add_argument("destination", type=Path,
                        help="new snapshot directory")
    parser.add_argument("--key-mode", choices=("inline", "external", "age"),
                        required=True)
    parser.add_argument("--recovery-key", type=Path)
    parser.add_argument("--dyappd", type=Path, required=True)
    parser.add_argument("--id-pow-bits", type=int, required=True)
    parser.add_argument("--age-identity", type=Path)
    parser.add_argument("--age-binary", type=Path)
    parser.add_argument("--scratch", type=Path,
                        help="directory for restored copies (default: TMPDIR)")
    args = parser.parse_args()
    try:
        with stopped(args.source):
            peer_id = preflight(args.source, args.destination, args.key_mode,
                                args.recovery_key, args.dyappd,
                                args.id_pow_bits, args.age_identity,
                                args.age_binary, args.scratch)
    except (OSError, sqlite3.Error, UnicodeError, ValueError,
            subprocess.TimeoutExpired) as error:
        parser.exit(1, f"update preflight failed: {error}\n")
    print(f"update preflight passed; peer ID: {peer_id}")


if __name__ == "__main__":
    main()
