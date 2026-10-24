#!/usr/bin/env python3
"""Prove a stopped-node snapshot's SQLite stores and recovered node key."""

import argparse
import importlib.util
import os
from pathlib import Path
import stat
import subprocess
import tempfile


SCRIPT = Path(__file__).with_name("probe-bootstrap-sqlite.py")
SPEC = importlib.util.spec_from_file_location("probe_bootstrap_sqlite", SCRIPT)
SQLITE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SQLITE)


def decrypt_age(encrypted, identity, output, age_binary):
    if identity.is_symlink():
        raise ValueError("age identity must not be a symlink")
    info = identity.stat()
    unsafe = not stat.S_ISREG(info.st_mode) or info.st_nlink != 1
    if unsafe or info.st_mode & 0o077:
        raise ValueError("age identity must be a private regular file")
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW
    fd = os.open(output, flags, 0o600)
    try:
        with os.fdopen(fd, "wb", closefd=False) as plaintext:
            result = subprocess.run(
                [str(age_binary), "--decrypt", "--identity", str(identity),
                 str(encrypted)], stdout=plaintext, stderr=subprocess.DEVNULL,
                stdin=subprocess.DEVNULL, timeout=60, check=False)
        if result.returncode:
            raise ValueError("age decryption failed")
    finally:
        os.close(fd)
    if not 0 < output.stat().st_size <= 4096:
        raise ValueError("decrypted node key has invalid size")


def probe(snapshot, dyappd, id_pow_bits, age_identity=None, age_binary=None,
          scratch=None):
    if not 0 <= id_pow_bits <= 32:
        raise ValueError("ID proof-of-work difficulty must be 0..32")
    header, records, _ = SQLITE.probe(snapshot, scratch)
    root = snapshot.resolve(strict=True)
    mode = header["key_mode"]
    if mode == "age" and (age_identity is None or age_binary is None):
        raise ValueError("age identity and binary are required")
    if mode != "age" and (age_identity is not None or age_binary is not None):
        raise ValueError("age options are only valid for age snapshots")
    with tempfile.TemporaryDirectory(prefix="dyappd-key-restore-",
                                     dir=scratch) as temporary:
        scratch = Path(temporary)
        for name in ("storage/node.id", "storage/network"):
            SQLITE.copy_verified_file(root, scratch, records[name])
        if mode == "age":
            source = "recovery/node.key.age"
            if records[source]["size"] > 65536:
                raise ValueError("encrypted key exceeds size limit")
            SQLITE.copy_verified_file(root, scratch, records[source])
            key = scratch / "storage/node.key"
            decrypt_age(scratch / source, age_identity, key, age_binary)
        else:
            source = ("storage/node.key" if mode == "inline"
                      else "recovery/node.key")
            if records[source]["size"] > 4096:
                raise ValueError("node key exceeds size limit")
            SQLITE.copy_verified_file(root, scratch, records[source])
            key = scratch / source
        config = scratch / "empty.toml"
        config.write_text("")
        command = [str(dyappd), "probe-key", "--config", str(config),
                   f"--storage.dir={scratch / 'storage'}",
                   f"--storage.key={key}",
                   f"--network.name={header['network']}",
                   f"--network.id-pow-bits={id_pow_bits}"]
        # Keep host credentials and DYAPPD_* overrides out of the probe.
        result = subprocess.run(command, stdout=subprocess.PIPE,
                                stderr=subprocess.DEVNULL,
                                stdin=subprocess.DEVNULL,
                                env={"LANG": "C"}, timeout=30, check=False)
        peer_id = result.stdout.decode("utf-8").strip()
        if result.returncode or peer_id != header["peer_id"]:
            raise ValueError("restored node key does not match saved peer ID")
    return header["peer_id"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("snapshot", type=Path)
    parser.add_argument("--dyappd", type=Path, required=True)
    parser.add_argument("--id-pow-bits", type=int, required=True)
    parser.add_argument("--age-identity", type=Path)
    parser.add_argument("--age-binary", type=Path)
    parser.add_argument("--scratch", type=Path,
                        help="directory for restored copies (default: TMPDIR)")
    args = parser.parse_args()
    try:
        peer_id = probe(args.snapshot, args.dyappd, args.id_pow_bits,
                        args.age_identity, args.age_binary, args.scratch)
    except (OSError, UnicodeError, ValueError,
            subprocess.TimeoutExpired) as error:
        parser.exit(1, f"backup preflight failed: {error}\n")
    print(f"backup preflight passed; peer ID: {peer_id}")


if __name__ == "__main__":
    main()
