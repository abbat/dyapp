#!/usr/bin/env python3
"""Read single files of a CI artifact with HTTP ranges, not the whole zip.

Usage: python3 -I -B artifact-member.py RUN_ID ARTIFACT_NAME OUT_DIR SUFFIX...
       python3 -I -B artifact-member.py --self-test
Needs gh authentication; gh follows the redirect and keeps the Range header.
Writes OUT_DIR/names.txt (size and path of every member) and each member whose
path ends in a SUFFIX, under a flat name. An artifact's content is untrusted:
OUT_DIR holds only data.
"""

import io
import json
import os
import struct
import subprocess
import sys
import zipfile
import zlib


REPO = "abbat/dyapp"
TAIL_BYTES = 1 << 21


def members(fetch, size):
    """(name, method, packed, size, offset) per central directory entry."""
    tail_start = max(0, size - TAIL_BYTES)
    tail = fetch(tail_start, size - 1)
    end = tail.rfind(b"PK\x05\x06")
    if end < 0:
        raise RuntimeError("no end of central directory in the tail")
    cd_size, cd_offset = struct.unpack("<II", tail[end + 12 : end + 20])
    # shortcut: no zip64, a member past 4 GiB stops here;
    # add it when an artifact gets there.
    if cd_offset == 0xFFFFFFFF:
        raise RuntimeError("zip64 archive")
    cd = (
        tail[cd_offset - tail_start :]
        if cd_offset >= tail_start
        else fetch(cd_offset, cd_offset + cd_size - 1)
    )
    found = []
    pos = 0
    while cd[pos : pos + 4] == b"PK\x01\x02":
        method = struct.unpack("<H", cd[pos + 10 : pos + 12])[0]
        packed, plain = struct.unpack("<II", cd[pos + 20 : pos + 28])
        name_len, extra_len, comment_len = struct.unpack(
            "<HHH", cd[pos + 28 : pos + 34]
        )
        offset = struct.unpack("<I", cd[pos + 42 : pos + 46])[0]
        name = cd[pos + 46 : pos + 46 + name_len].decode("utf-8", "replace")
        found.append((name, method, packed, plain, offset))
        pos += 46 + name_len + extra_len + comment_len
    return found


def content(fetch, member):
    """A member's bytes; the local header has its own lengths."""
    _, method, packed, _, offset = member
    name_len, extra_len = struct.unpack("<HH", fetch(offset + 26, offset + 29))
    start = offset + 30 + name_len + extra_len
    data = fetch(start, start + packed - 1) if packed else b""
    return zlib.decompress(data, -15) if method == 8 else data


def self_test():
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("a/b/tests.log", "hello " * 100)
        archive.writestr("a/empty.log", "")
    raw = buffer.getvalue()

    def fetch(start, end):
        return raw[start : end + 1]

    found = {m[0]: m for m in members(fetch, len(raw))}
    assert sorted(found) == ["a/b/tests.log", "a/empty.log"]
    assert content(fetch, found["a/b/tests.log"]) == b"hello " * 100
    assert content(fetch, found["a/empty.log"]) == b""
    print("artifact-member: self-test passed")


def main():
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return
    run, artifact, out = sys.argv[1:4]
    suffixes = tuple(sys.argv[4:])
    listing = json.loads(
        subprocess.check_output(
            [
                "gh",
                "api",
                f"repos/{REPO}/actions/runs/{run}/artifacts?per_page=100",
            ]
        )
    )
    chosen = next(a for a in listing["artifacts"] if a["name"] == artifact)

    def fetch(start, end):
        return subprocess.check_output(
            [
                "gh",
                "api",
                "-H",
                f"Range: bytes={start}-{end}",
                f"repos/{REPO}/actions/artifacts/{chosen['id']}/zip",
            ]
        )

    found = members(fetch, chosen["size_in_bytes"])
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "names.txt"), "w", encoding="utf-8") as names:
        for member in found:
            names.write(f"{member[3]} {member[0]}\n")
    for index, member in enumerate(found):
        if suffixes and member[0].endswith(suffixes):
            path = os.path.join(out, f"{index}-{os.path.basename(member[0])}")
            with open(path, "wb") as target:
                target.write(content(fetch, member))
            print(path, member[3], member[0])


if __name__ == "__main__":
    main()
