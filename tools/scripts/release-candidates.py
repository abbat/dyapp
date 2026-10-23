#!/usr/bin/env python3
"""Extract untrusted client candidates from a GitHub Release response.

Usage: release-candidates.py RELEASE.json TAG COMMIT
Input is the REST get-by-tag response. Output is unverified.
"""

import json
from pathlib import Path
import re
import sys


TAG = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")
SHA = re.compile(r"[0-9a-f]{40}\Z")
DIGEST = re.compile(r"sha256:([0-9a-f]{64})\Z")
MAX_BYTES = 1024 * 1024


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def stable_tag(tag):
    if not isinstance(tag, str) or len(tag) > 64:
        raise ValueError("invalid tag")
    match = TAG.fullmatch(tag)
    if match is None or any(int(part) > 2**31 - 1 for part in match.groups()):
        raise ValueError("invalid tag")


def candidates(release, tag, commit):
    stable_tag(tag)
    if not isinstance(commit, str) or not SHA.fullmatch(commit):
        raise ValueError("invalid commit")
    if not isinstance(release, dict) or release.get("tag_name") != tag:
        raise ValueError("release tag mismatch")
    if release.get("draft") is not False:
        raise ValueError("release is a draft")
    if release.get("prerelease") is not False:
        raise ValueError("release is a prerelease")
    if not release.get("published_at") or not isinstance(
        release["published_at"], str
    ):
        raise ValueError("release is not published")
    assets = release.get("assets")
    if not isinstance(assets, list) or len(assets) > 100:
        raise ValueError("invalid release assets")
    version = tag[1:]
    expected = {
        f"dyapp_{version}_amd64.deb": "linux-x86_64",
        f"dyapp-{version}-x86_64-windows.zip": "windows-x86_64",
    }
    packages = {}
    names = set()
    for asset in assets:
        if not isinstance(asset, dict):
            raise ValueError("invalid release asset")
        name = asset.get("name")
        if not isinstance(name, str) or not name or len(name) > 160:
            raise ValueError("invalid release asset name")
        if name in names:
            raise ValueError("duplicate release asset")
        names.add(name)
        if name.startswith("dyapp_") and name.endswith("_amd64.deb"):
            if name not in expected:
                raise ValueError("wrong-version client package")
        if name.startswith("dyapp-") and name.endswith("-windows.zip"):
            if name not in expected:
                raise ValueError("wrong-version client package")
        target = expected.get(name)
        if target is None:
            continue
        digest = asset.get("digest")
        size = asset.get("size")
        if asset.get("state") != "uploaded":
            raise ValueError("client package not uploaded")
        if not isinstance(digest, str) or not DIGEST.fullmatch(digest):
            raise ValueError("client package has no SHA-256 digest")
        if type(size) is not int or not 0 < size <= 1024**3:
            raise ValueError("invalid client package size")
        packages[target] = {
            "name": name, "sha256": digest[7:], "size": size,
        }
    return {
        "schema": 1, "tag": tag, "commit": commit, "packages": packages,
    }


def main():
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        data = Path(sys.argv[1]).read_bytes()
        if len(data) > MAX_BYTES:
            raise ValueError("release JSON exceeds size limit")
        release = json.loads(data, object_pairs_hook=unique_object)
        result = candidates(release, sys.argv[2], sys.argv[3])
    except (OSError, ValueError, TypeError) as error:
        print(f"release candidates rejected: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
