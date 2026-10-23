#!/usr/bin/env python3
"""Copy a stopped bootstrap node into a private, manifest-backed directory.

This writer does not prove a backup is restorable. The independent verifier and
automatic-update gate are separate checkpoints. Never point it at a live node.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat


REQUIRED = {"node.id", "network", "profiles.db", "messages.db"}
OPTIONAL = {"media.db", "deny.db", "data"}
DATABASES = {"profiles.db", "messages.db", "media.db", "deny.db"}
DISPOSABLE = {"search.lock", "peers", "anchors", "traffic"}
INDEX = re.compile(r"profiles-idx(?:-v[0-9]+)?\.db(?:-(?:wal|shm|journal))?\Z")
MAX_FILES = 1_000_000
MAX_MANIFEST = 256 * 1024 * 1024
MAX_RECORD = 4096
MAX_PATH = 1024


def regular_file(path):
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        raise ValueError(f"not a private regular file: {path}")
    return info


def paths(source, key_mode):
    root = {}
    for item in source.iterdir():
        name = item.name
        if name in REQUIRED or name in DATABASES or name == "node.key":
            regular_file(item)
            root[name] = item
        elif name == "data":
            if not stat.S_ISDIR(item.lstat().st_mode):
                raise ValueError("data is not a directory")
            root[name] = item
        elif any(name == f"{db}{suffix}" for db in DATABASES
                 for suffix in ("-wal", "-journal", "-shm")):
            regular_file(item)
            root[name] = item
        elif name in DISPOSABLE or INDEX.fullmatch(name):
            continue
        else:
            raise ValueError(f"unknown storage entry: {name}")
    missing = REQUIRED - root.keys()
    if missing:
        raise ValueError(f"missing required storage files: {sorted(missing)}")
    if key_mode == "inline" and "node.key" not in root:
        raise ValueError("missing inline node.key")
    if key_mode != "inline" and "node.key" in root:
        raise ValueError("conflicting inline node.key")
    if "data" in root and "media.db" not in root:
        raise ValueError("data exists without media.db")
    for db in DATABASES:
        for suffix in ("-wal", "-journal", "-shm"):
            if f"{db}{suffix}" in root and db not in root:
                raise ValueError(f"orphaned SQLite sidecar: {db}{suffix}")

    result = [(f"storage/{name}", item) for name, item in root.items()
              if name != "data" and not name.endswith("-shm")]
    if "data" in root:
        stack = [root["data"]]
        directories = 0
        while stack:
            directories += 1
            if directories > MAX_FILES:
                raise ValueError("too many directories")
            directory = stack.pop()
            for item in directory.iterdir():
                mode = item.lstat().st_mode
                if stat.S_ISDIR(mode):
                    stack.append(item)
                else:
                    regular_file(item)
                    relative = "storage/" + item.relative_to(source).as_posix()
                    result.append((relative, item))
                if len(result) > MAX_FILES:
                    raise ValueError("too many files")
    return result, sorted(OPTIONAL - root.keys())


def copy_file(source, target):
    fd = os.open(source, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
            raise ValueError(f"not a private regular file: {source}")
        target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW
        out = os.open(target, flags, 0o600)
        digest = hashlib.sha256()
        size = 0
        try:
            while block := os.read(fd, 1024 * 1024):
                digest.update(block)
                size += len(block)
                view = memoryview(block)
                while view:
                    written = os.write(out, view)
                    if written == 0:
                        raise OSError("short backup write")
                    view = view[written:]
            os.fsync(out)
        finally:
            os.close(out)
        after = os.fstat(fd)
        same_size = size == before.st_size == after.st_size
        same_time = before.st_mtime_ns == after.st_mtime_ns
        if not same_size or not same_time:
            raise ValueError(f"source changed while copying: {source}")
        return {"path": target, "size": size, "sha256": digest.hexdigest()}
    finally:
        os.close(fd)


def write_manifest(destination, header, files):
    pending = destination / "manifest.pending"
    total = 0
    with pending.open("x", encoding="utf-8") as output:
        def write_record(record):
            nonlocal total
            line = json.dumps(record, ensure_ascii=False, sort_keys=True,
                              separators=(",", ":")) + "\n"
            encoded = line.encode("utf-8")
            too_long = len(encoded) > MAX_RECORD
            too_large = total + len(encoded) > MAX_MANIFEST
            if too_long or too_large:
                raise ValueError("manifest exceeds size limit")
            output.write(line)
            total += len(encoded)

        write_record(header)
        for relative, original in files:
            if len(relative.encode("utf-8")) > MAX_PATH:
                raise ValueError("path exceeds size limit")
            target = destination / relative
            record = copy_file(original, target)
            record["path"] = relative
            write_record(record)
        output.flush()
        os.fsync(output.fileno())
    os.replace(pending, destination / "manifest.jsonl")


def snapshot(source, destination, key_mode, recovery_key):
    if source.is_symlink():
        raise ValueError("source must not be a symlink")
    source = source.resolve(strict=True)
    if destination.is_symlink():
        raise ValueError("destination must not be a symlink")
    destination = destination.resolve(strict=False)
    if not stat.S_ISDIR(source.lstat().st_mode):
        raise ValueError("source is not a directory")
    if destination == source or source in destination.parents:
        raise ValueError("destination must be outside source")
    if destination.exists():
        raise ValueError("destination already exists")
    if key_mode == "inline" and recovery_key is not None:
        raise ValueError("inline mode does not take a recovery key")
    if key_mode != "inline" and recovery_key is None:
        raise ValueError("portable recovery key file is required")

    files, absent = paths(source, key_mode)
    if (source / "node.id").stat().st_size > 256:
        raise ValueError("node identity marker exceeds size limit")
    if (source / "network").stat().st_size > 64:
        raise ValueError("network marker exceeds size limit")
    peer_id = (source / "node.id").read_text(encoding="utf-8").strip()
    network = (source / "network").read_text(encoding="utf-8")
    if not peer_id or len(peer_id) > 256 or len(network.encode("utf-8")) > 64:
        raise ValueError("invalid node identity or network marker")
    if recovery_key is not None:
        regular_file(recovery_key)
        name = "node.key.age" if key_mode == "age" else "node.key"
        files.append((f"recovery/{name}", recovery_key))
    files.sort(key=lambda entry: entry[0])
    if len(files) > MAX_FILES:
        raise ValueError("too many files")

    destination.mkdir(mode=0o700)
    (destination / "storage").mkdir(mode=0o700)
    (destination / "recovery").mkdir(mode=0o700)
    if "data" not in absent:
        (destination / "storage/data").mkdir(mode=0o700)
    header = {"schema": 1, "peer_id": peer_id, "network": network,
              "key_mode": key_mode, "absent": absent,
              "hot_journal": any(name.endswith(".db-journal")
                                 for name, _ in files)}
    write_manifest(destination, header, files)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--key-mode", choices=("inline", "external", "age"),
                        required=True)
    parser.add_argument("--recovery-key", type=Path)
    args = parser.parse_args()
    try:
        snapshot(args.source, args.destination, args.key_mode,
                 args.recovery_key)
    except (OSError, UnicodeError, ValueError) as error:
        parser.exit(1, f"backup failed: {error}\n")


if __name__ == "__main__":
    main()
