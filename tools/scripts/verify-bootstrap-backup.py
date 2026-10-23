#!/usr/bin/env python3
"""Check a stopped-node snapshot's manifest and exact file inventory.

This verifies bytes and paths, not SQLite recovery or the node key. Those
probes are separate checkpoints. It never opens a live storage directory.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat


SCRIPT = Path(__file__).with_name("backup-bootstrap.py")
SPEC = importlib.util.spec_from_file_location("backup_bootstrap", SCRIPT)
BACKUP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BACKUP)
DIGEST = re.compile(r"[0-9a-f]{64}\Z")
HEADER_FIELDS = {"schema", "peer_id", "network", "key_mode", "absent",
                 "hot_journal"}
FILE_FIELDS = {"path", "size", "sha256"}


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def open_beneath(root, relative):
    parts = PurePosixPath(relative).parts
    directory = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        for part in parts[:-1]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                            dir_fd=directory)
            os.close(directory)
            directory = child
        return os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW,
                       dir_fd=directory)
    finally:
        os.close(directory)


def read_marker(root, name, limit):
    fd = open_beneath(root, f"storage/{name}")
    try:
        value = os.read(fd, limit + 1)
        if len(value) > limit or os.read(fd, 1):
            raise ValueError(f"{name} exceeds size limit")
        return value.decode("utf-8")
    finally:
        os.close(fd)


def check_file(root, record):
    fd = open_beneath(root, record["path"])
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
            raise ValueError(f"unsafe file: {record['path']}")
        digest = hashlib.sha256()
        size = 0
        while block := os.read(fd, 1024 * 1024):
            digest.update(block)
            size += len(block)
        after = os.fstat(fd)
        if any((size != record["size"], size != before.st_size,
                before.st_size != after.st_size)):
            raise ValueError(f"size mismatch: {record['path']}")
        if before.st_mtime_ns != after.st_mtime_ns:
            raise ValueError(f"file changed while checking: {record['path']}")
        if digest.hexdigest() != record["sha256"]:
            raise ValueError(f"hash mismatch: {record['path']}")
    finally:
        os.close(fd)


def validate_path(value, key_mode):
    if not isinstance(value, str) or not value:
        raise ValueError("invalid manifest path")
    if len(value.encode("utf-8")) > BACKUP.MAX_PATH:
        raise ValueError("manifest path exceeds size limit")
    path = PurePosixPath(value)
    if any((path.is_absolute(), path.as_posix() != value,
            len(path.parts) < 2,
            any(part in (".", "..") for part in path.parts))):
        raise ValueError("unsafe manifest path")
    parts = path.parts
    if parts[0] == "recovery":
        expected = "node.key.age" if key_mode == "age" else "node.key"
        if key_mode == "inline" or parts != ("recovery", expected):
            raise ValueError("unexpected recovery key path")
    elif parts[0] == "storage":
        if len(parts) >= 3 and parts[1] == "data":
            return parts
        if len(parts) != 2:
            raise ValueError("unknown storage path")
        name = parts[1]
        files = BACKUP.REQUIRED | (BACKUP.OPTIONAL - {"data"})
        if (name == "node.key" and key_mode == "inline") or name in files:
            return parts
        if any(name == f"{db}{suffix}" for db in BACKUP.DATABASES
               for suffix in ("-wal", "-journal")):
            return parts
        raise ValueError("unknown storage path")
    else:
        raise ValueError("unknown manifest root")
    return parts


def header_record(value):
    if not isinstance(value, dict) or set(value) != HEADER_FIELDS:
        raise ValueError("invalid backup header")
    if type(value["schema"]) is not int or value["schema"] != 1:
        raise ValueError("unsupported backup schema")
    if not isinstance(value["peer_id"], str):
        raise ValueError("invalid peer ID")
    if not 1 <= len(value["peer_id"]) <= 256:
        raise ValueError("invalid peer ID")
    if not isinstance(value["network"], str):
        raise ValueError("invalid network marker")
    if len(value["network"].encode("utf-8")) > 64:
        raise ValueError("invalid network marker")
    if not isinstance(value["key_mode"], str) or value["key_mode"] not in (
            "inline", "external", "age"):
        raise ValueError("invalid key mode")
    absent = value["absent"]
    if not isinstance(absent, list) or any(
            not isinstance(item, str) for item in absent):
        raise ValueError("invalid absent-store list")
    if absent != sorted(set(absent)) or not set(absent) <= BACKUP.OPTIONAL:
        raise ValueError("invalid absent-store list")
    if type(value["hot_journal"]) is not bool:
        raise ValueError("invalid hot-journal flag")
    return value


def file_record(value, previous, key_mode):
    path = value["path"]
    parts = validate_path(path, key_mode)
    if path <= previous:
        raise ValueError("manifest paths are not sorted and unique")
    size = value["size"]
    if type(size) is not int or not 0 <= size <= 2**63 - 1:
        raise ValueError("invalid file size")
    if not isinstance(value["sha256"], str) or not DIGEST.fullmatch(
            value["sha256"]):
        raise ValueError("invalid file digest")
    return parts


def parse_and_hash(root):
    fd = open_beneath(root, "manifest.jsonl")
    try:
        metadata = os.fstat(fd)
        if any((not stat.S_ISREG(metadata.st_mode), metadata.st_nlink != 1,
                metadata.st_size > BACKUP.MAX_MANIFEST)):
            raise ValueError("unsafe or oversized manifest")
        with os.fdopen(fd, "rb", closefd=False) as stream:
            def record():
                line = stream.readline(BACKUP.MAX_RECORD + 1)
                if any((not line, len(line) > BACKUP.MAX_RECORD,
                        not line.endswith(b"\n"))):
                    raise ValueError("invalid or oversized manifest record")
                return json.loads(line, object_pairs_hook=unique_object)

            header = header_record(record())
            key_mode = header["key_mode"]
            expected_dirs = {"storage", "recovery"}
            if "data" not in header["absent"]:
                expected_dirs.add("storage/data")
            seen = set()
            sidecars = set()
            previous = ""
            count = 0
            hot_journal = False
            recovery_key_seen = False
            while stream.peek(1):
                value = record()
                if not isinstance(value, dict) or set(value) != FILE_FIELDS:
                    raise ValueError("invalid file record")
                path = value["path"]
                parts = file_record(value, previous, key_mode)
                if count >= BACKUP.MAX_FILES:
                    raise ValueError("too many manifest files")
                check_file(root, value)
                count += 1
                previous = path
                if parts[0] == "recovery":
                    recovery_key_seen = True
                if parts[0] == "storage" and len(parts) == 2:
                    seen.add(parts[1])
                    for suffix in ("-wal", "-journal"):
                        if parts[1].endswith(suffix):
                            sidecars.add(parts[1][:-len(suffix)])
                for parent in PurePosixPath(path).parents:
                    if parent == PurePosixPath("."):
                        break
                    expected_dirs.add(parent.as_posix())
                hot_journal |= path.endswith(".db-journal")
            if metadata.st_size != stream.tell():
                raise ValueError("manifest changed while checking")
    finally:
        os.close(fd)
    required = set(BACKUP.REQUIRED)
    if key_mode == "inline":
        required.add("node.key")
    if not required <= seen:
        raise ValueError("missing required storage file")
    if not sidecars <= seen:
        raise ValueError("orphaned SQLite sidecar")
    for optional in BACKUP.OPTIONAL - {"data"}:
        if (optional in seen) == (optional in header["absent"]):
            raise ValueError("absent-store list does not match files")
    if "data" not in header["absent"] and "media.db" not in seen:
        raise ValueError("data exists without media.db")
    if hot_journal != header["hot_journal"]:
        raise ValueError("hot-journal flag does not match files")
    if recovery_key_seen != (key_mode != "inline"):
        raise ValueError("recovery key does not match key mode")
    if read_marker(root, "node.id", 256).strip() != header["peer_id"]:
        raise ValueError("peer ID differs from manifest")
    if read_marker(root, "network", 64) != header["network"]:
        raise ValueError("network differs from manifest")
    return header, count, expected_dirs


def check_inventory(root, expected_count, expected_dirs):
    def fail_walk(error):
        raise error

    found = 0
    actual_dirs = set()
    for directory, dirs, files in os.walk(root, followlinks=False,
                                          onerror=fail_walk):
        parent = Path(directory).relative_to(root)
        for name in dirs:
            relative = (parent / name).as_posix()
            path = Path(directory) / name
            if any((relative not in expected_dirs,
                    not stat.S_ISDIR(path.lstat().st_mode))):
                raise ValueError(f"unexpected directory: {relative}")
            actual_dirs.add(relative)
        for name in files:
            relative = (parent / name).as_posix()
            if relative == "manifest.jsonl":
                continue
            if parent == Path("."):
                raise ValueError(f"unexpected root file: {relative}")
            path = Path(directory) / name
            info = path.lstat()
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                raise ValueError(f"unsafe file: {relative}")
            found += 1
    if found != expected_count:
        raise ValueError("backup has extra files")
    if actual_dirs != expected_dirs:
        raise ValueError("backup has missing directories")


def verify(snapshot):
    if snapshot.is_symlink():
        raise ValueError("snapshot must not be a symlink")
    root = snapshot.resolve(strict=True)
    if not root.is_dir():
        raise ValueError("snapshot is not a directory")
    header, count, expected_dirs = parse_and_hash(root)
    check_inventory(root, count, expected_dirs)
    return header["peer_id"], count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("snapshot", type=Path)
    args = parser.parse_args()
    try:
        peer_id, count = verify(args.snapshot)
    except (OSError, UnicodeError, ValueError) as error:
        parser.exit(1, f"backup verification failed: {error}\n")
    print(f"snapshot files verified: {count}; peer ID: {peer_id}")


if __name__ == "__main__":
    main()
