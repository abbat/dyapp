#!/usr/bin/env python3
"""Replace a stopped node's storage with a verified pre-update snapshot."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import time


SCRIPT = Path(__file__).with_name("preflight-bootstrap-update.py")
SPEC = importlib.util.spec_from_file_location("preflight_update", SCRIPT)
PREFLIGHT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREFLIGHT)
PROBE = PREFLIGHT.PROBE
SQLITE = PROBE.SQLITE


def storage_records(snapshot):
    with (snapshot / "manifest.jsonl").open("rb") as manifest:
        header = json.loads(manifest.readline())
        records = [json.loads(line) for line in manifest]
    return header, [record for record in records
                    if record["path"].startswith("storage/")]


def own(path, owner):
    info = path.lstat()
    if (info.st_uid, info.st_gid) != (owner.st_uid, owner.st_gid):
        os.chown(path, owner.st_uid, owner.st_gid, follow_symlinks=False)


def sync(path):
    fd = os.open(path, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def stage(snapshot, staging, header, records, owner):
    """Copy and re-hash the snapshot's storage into `staging/storage`."""
    staging.mkdir(mode=0o700)
    (staging / "storage").mkdir(mode=0o700)
    if "data" not in header["absent"]:
        (staging / "storage/data").mkdir(mode=0o700)
    for record in records:
        SQLITE.copy_verified_file(snapshot, staging, record)
    for directory, _, files in os.walk(staging / "storage", topdown=False):
        for name in files:
            own(Path(directory, name), owner)
            sync(Path(directory, name))
        own(Path(directory), owner)
        sync(directory)


def restore(snapshot, storage, dyappd, id_pow_bits, age_identity=None,
            age_binary=None, scratch=None, free=PREFLIGHT.available):
    """Run inside `stopped(storage)`; returns the moved-aside storage path."""
    if storage.is_symlink() or snapshot.is_symlink():
        raise ValueError("storage and snapshot must not be symlinks")
    storage = storage.resolve(strict=True)
    snapshot = snapshot.resolve(strict=True)
    scratch = Path(scratch or tempfile.gettempdir())
    peer_id = PROBE.probe(snapshot, dyappd, id_pow_bits, age_identity,
                          age_binary, scratch)
    header, records = storage_records(snapshot)
    marker = storage / "node.id"
    if marker.exists() and marker.read_text().strip() != peer_id:
        raise ValueError("snapshot belongs to another node")
    size = sum(record["size"] for record in records)
    if free(storage.parent) < size + PREFLIGHT.MARGIN:
        raise ValueError(f"not enough free space on {storage.parent}: "
                         f"need {size + PREFLIGHT.MARGIN} bytes")
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    staging = storage.with_name(f".{storage.name}.restore-{stamp}")
    failed = storage.with_name(f"{storage.name}.failed-{stamp}")
    if failed.exists():
        raise ValueError(f"{failed} already exists")
    owner = storage.stat()
    try:
        stage(snapshot, staging, header, records, owner)
        os.chmod(staging / "storage", owner.st_mode & 0o7777)
        # The restored directory gets its own held lock before it goes live.
        with PREFLIGHT.stopped(staging / "storage"):
            os.rename(storage, failed)
            try:
                os.rename(staging / "storage", storage)
            except BaseException:
                os.rename(failed, storage)
                raise
            sync(storage.parent)
    finally:
        # Only a partial copy or an empty staging directory is left here.
        shutil.rmtree(staging, ignore_errors=True)
    return failed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("snapshot", type=Path,
                        help="snapshot made by preflight-bootstrap-update.py")
    parser.add_argument("storage", type=Path, help="dyappd storage.dir")
    parser.add_argument("--dyappd", type=Path, required=True)
    parser.add_argument("--id-pow-bits", type=int, required=True)
    parser.add_argument("--age-identity", type=Path)
    parser.add_argument("--age-binary", type=Path)
    parser.add_argument("--scratch", type=Path,
                        help="directory for probe copies (default: TMPDIR)")
    args = parser.parse_args()
    try:
        with PREFLIGHT.stopped(args.storage):
            failed = restore(args.snapshot, args.storage, args.dyappd,
                             args.id_pow_bits, args.age_identity,
                             args.age_binary, args.scratch)
    except (OSError, sqlite3.Error, UnicodeError, ValueError,
            subprocess.TimeoutExpired) as error:
        parser.exit(1, f"restore failed: {error}\n")
    print(f"restored {args.storage}; previous storage kept in {failed}")


if __name__ == "__main__":
    main()
