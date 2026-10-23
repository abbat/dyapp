#!/usr/bin/env python3
"""Verify published client packages and emit trusted feed inventory.

Usage: verify-release-packages.py TAG > inventory.json
Run from the checked-out release tag with gh authentication.
"""

import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "release_candidates", ROOT / "tools/scripts/release-candidates.py")
CANDIDATES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CANDIDATES)
REPO = "abbat/dyapp"
SIGNER = "abbat/dyapp/.github/workflows/release.yml"


def command(args, timeout=120):
    try:
        return subprocess.run(
            args, check=True, capture_output=True, timeout=timeout
        ).stdout
    except (OSError, subprocess.CalledProcessError,
            subprocess.TimeoutExpired) as error:
        raise ValueError(f"verification command failed: {args[0]}") from error


def release_commit(tag, run=command):
    CANDIDATES.stable_tag(tag)
    commit = run(["git", "rev-parse", f"refs/tags/{tag}^{{commit}}"])
    commit = commit.decode("ascii").strip()
    head = run(["git", "rev-parse", "HEAD"]).decode("ascii").strip()
    if commit != head:
        raise ValueError("checkout does not match release tag")
    if not CANDIDATES.SHA.fullmatch(commit):
        raise ValueError("invalid tag commit")
    return commit


def digest_file(path):
    sha = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest()


def verify_package(package, tag, commit, directory, run=command):
    name = package["name"]
    run([
        "gh", "release", "download", tag, "-R", REPO,
        "-p", name, "-D", str(directory),
    ], timeout=600)
    path = directory / name
    if not path.is_file() or path.stat().st_size != package["size"]:
        raise ValueError("downloaded package size mismatch")
    if digest_file(path) != package["sha256"]:
        raise ValueError("downloaded package digest mismatch")
    output = run([
        "gh", "attestation", "verify", str(path),
        "--repo", REPO, "--signer-workflow", SIGNER,
        "--source-ref", f"refs/tags/{tag}", "--source-digest", commit,
        "--deny-self-hosted-runners", "--format", "json",
    ], timeout=120)
    results = json.loads(output, object_pairs_hook=CANDIDATES.unique_object)
    if not isinstance(results, list) or not any(
        isinstance(row, dict) and row.get("verificationResult")
        for row in results
    ):
        raise ValueError("no verified package attestation")


def inventory(tag, run=command):
    commit = release_commit(tag, run)
    response = run([
        "gh", "api", f"repos/{REPO}/releases/tags/{tag}",
    ])
    if len(response) > CANDIDATES.MAX_BYTES:
        raise ValueError("release response exceeds size limit")
    release = json.loads(response, object_pairs_hook=CANDIDATES.unique_object)
    found = CANDIDATES.candidates(release, tag, commit)
    assets = []
    attestations = []
    targets = {}
    with tempfile.TemporaryDirectory(prefix="dyapp-release-") as temporary:
        directory = Path(temporary)
        for target, package in sorted(found["packages"].items()):
            verify_package(package, tag, commit, directory, run)
            asset = {
                "name": package["name"], "state": "uploaded",
                "sha256": package["sha256"],
            }
            assets.append(asset)
            attestations.append({
                "name": package["name"], "sha256": package["sha256"],
            })
            targets[target] = {
                "method": "package", "asset": package["name"],
            }
    return {"schema": 1, "releases": [{
        "tag": tag, "commit": commit, "draft": False,
        "prerelease": False, "assets": assets,
        "attestations": attestations, "source_builds": [],
        "targets": targets,
    }]}


def main():
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        result = inventory(sys.argv[1])
    except (ValueError, UnicodeError, KeyError, TypeError) as error:
        print(f"release verification rejected: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
