#!/usr/bin/env python3
"""Verify published client packages and emit trusted feed inventory.

Usage: verify-release-packages.py TAG > inventory.json
Run from the checked-out release tag with gh authentication.
Android inventory additionally requires the validated prior feed supplied by
stage-update-feed.py; direct invocation cannot establish that baseline.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import TYPE_CHECKING, cast

import hashlib
import importlib.util
import json
import os
import types
from pathlib import Path
import subprocess
import sys
import tempfile


if TYPE_CHECKING:
    import release_contracts as rc

ROOT = Path(__file__).resolve().parents[2]


def load_module(script: str) -> types.ModuleType:
    name = Path(script).stem.replace("-", "_")
    spec = importlib.util.spec_from_file_location(
        name, ROOT / "tools/scripts" / script
    )
    assert spec is not None and spec.loader is not None, (
        "Python source loader is unavailable"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CANDIDATES = cast("rc.CandidateModule", load_module("release-candidates.py"))
ANDROID = cast(
    "rc.AndroidPackageModule", load_module("android-release-package.py")
)
BUILD = cast("rc.FeedModule", load_module("build-update-feed.py"))
REPO = "abbat/dyapp"
SIGNER = "abbat/dyapp/.github/workflows/release.yml"


def command(args: Sequence[str], timeout: int = 120) -> bytes:
    try:
        return subprocess.run(
            args, check=True, capture_output=True, timeout=timeout
        ).stdout
    except (
        OSError,
        subprocess.CalledProcessError,
        subprocess.TimeoutExpired,
    ) as error:
        raise ValueError(f"verification command failed: {args[0]}") from error


def tag_commit(tag: str, run: rc.CommandRunner) -> str:
    CANDIDATES.stable_tag(tag)
    commit = (
        run(["git", "rev-parse", f"refs/tags/{tag}^{{commit}}"])
        .decode("ascii")
        .strip()
    )
    if not CANDIDATES.SHA.fullmatch(commit):
        raise ValueError("invalid tag commit")
    return commit


def release_commit(tag: str, run: rc.CommandRunner = command) -> str:
    commit = tag_commit(tag, run)
    head = run(["git", "rev-parse", "HEAD"]).decode("ascii").strip()
    if commit != head:
        raise ValueError("checkout does not match release tag")
    return commit


def digest_file(path: Path) -> str:
    sha = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest()


def verify_package(
    package: rc.Package,
    tag: str,
    commit: str,
    directory: Path,
    run: rc.CommandRunner = command,
) -> None:
    name = package["name"]
    run(
        [
            "gh",
            "release",
            "download",
            tag,
            "-R",
            REPO,
            "-p",
            name,
            "-D",
            str(directory),
        ],
        timeout=600,
    )
    path = directory / name
    if not path.is_file() or path.stat().st_size != package["size"]:
        raise ValueError("downloaded package size mismatch")
    if digest_file(path) != package["sha256"]:
        raise ValueError("downloaded package digest mismatch")
    output = run(
        [
            "gh",
            "attestation",
            "verify",
            str(path),
            "--repo",
            REPO,
            "--signer-workflow",
            SIGNER,
            "--source-ref",
            f"refs/tags/{tag}",
            "--source-digest",
            commit,
            "--deny-self-hosted-runners",
            "--format",
            "json",
        ],
        timeout=120,
    )
    results = json.loads(output, object_pairs_hook=CANDIDATES.unique_object)
    if not isinstance(results, list) or not any(
        isinstance(row, dict) and row.get("verificationResult")
        for row in results
    ):
        raise ValueError("no verified package attestation")


def published_candidates(
    tag: str, commit: str, run: rc.CommandRunner
) -> rc.Candidates:
    response = run(["gh", "api", f"repos/{REPO}/releases/tags/{tag}"])
    if len(response) > CANDIDATES.MAX_BYTES:
        raise ValueError("release response exceeds size limit")
    release = json.loads(response, object_pairs_hook=CANDIDATES.unique_object)
    return CANDIDATES.candidates(release, tag, commit)


def verify_android(
    package: rc.Package,
    tag: str,
    directory: Path,
    previous: rc.Feed | None,
    certificate: str,
    run: rc.CommandRunner,
) -> None:
    if previous is None:
        raise ValueError(
            "Android inventory requires a validated previous feed"
        )
    current = ANDROID.metadata(
        directory / package["name"], tag[1:], certificate, run
    )
    prior = previous["targets"].get("android")
    if prior is None:
        return
    if prior["method"] != "package":
        raise ValueError("Android delivery migration is not verified")
    previous_tag = prior["tag"]
    if previous_tag == tag:
        return
    commit = tag_commit(previous_tag, run)
    candidates = published_candidates(previous_tag, commit, run)
    older = candidates["packages"].get("android")
    if older is None:
        raise ValueError("previous Android package is unavailable")
    previous_directory = directory / "previous-android"
    previous_directory.mkdir()
    verify_package(older, previous_tag, commit, previous_directory, run)
    baseline = ANDROID.metadata(
        previous_directory / older["name"], previous_tag[1:], certificate, run
    )
    ANDROID.verify_upgrade(current, baseline)


def inventory(
    tag: str,
    run: rc.CommandRunner = command,
    *,
    previous: rc.Feed | None = None,
    certificate: str | None = None,
) -> rc.Inventory:
    commit = release_commit(tag, run)
    found = published_candidates(tag, commit, run)
    assets: list[rc.Asset] = []
    attestations: list[rc.Attestation] = []
    targets: dict[str, rc.Claim] = {}
    with tempfile.TemporaryDirectory(prefix="dyapp-release-") as temporary:
        directory = Path(temporary)
        for target, package in sorted(found["packages"].items()):
            verify_package(package, tag, commit, directory, run)
            if target == "android":
                pin = (
                    os.environ.get("ANDROID_RELEASE_CERT_SHA256", "")
                    if certificate is None
                    else certificate
                )
                verify_android(package, tag, directory, previous, pin, run)
            asset: rc.Asset = {
                "name": package["name"],
                "state": "uploaded",
                "sha256": package["sha256"],
            }
            assets.append(asset)
            attestations.append(
                {
                    "name": package["name"],
                    "sha256": package["sha256"],
                }
            )
            targets[target] = {
                "method": "package",
                "asset": package["name"],
            }
    return {
        "schema": 1,
        "releases": [
            {
                "tag": tag,
                "commit": commit,
                "draft": False,
                "prerelease": False,
                "assets": assets,
                "attestations": attestations,
                "source_builds": [],
                "targets": targets,
            }
        ],
    }


def main() -> "int":
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
