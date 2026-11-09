#!/usr/bin/env python3
"""Build a stable client update feed from CI-verified release inventory.

Usage: build-update-feed.py INVENTORY.json [PREVIOUS.json] > stable.json
The inventory is trusted CI input; it checks internal consistency,
not GitHub attestations or store listings. It writes nothing on failure.
"""

from __future__ import annotations

from collections.abc import Container, Iterable, Mapping, Sequence
from typing import TYPE_CHECKING, cast

import json
from pathlib import Path
import re
import sys


if TYPE_CHECKING:
    import release_contracts as rc

TARGETS = {
    "linux-x86_64",
    "windows-x86_64",
    "macos-universal",
    "android",
    "ios",
}
SOURCE_TARGETS = {"macos-universal", "ios"}
VERSION = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")
DIGEST = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
MAX_COMPONENT = 2**31 - 1


def unique_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def read_json(path: str | Path, limit: int) -> object:
    data = Path(path).read_bytes()
    if len(data) > limit:
        raise ValueError("input exceeds size limit")
    value: object = json.loads(data, object_pairs_hook=unique_object)
    return value


def version(tag: "object") -> "tuple[int, ...]":
    if not isinstance(tag, str) or len(tag) > 64:
        raise ValueError("invalid stable tag")
    match = VERSION.fullmatch(tag)
    if not match:
        raise ValueError("invalid stable tag")
    parts = tuple(map(int, match.groups()))
    if any(part > MAX_COMPONENT for part in parts):
        raise ValueError("version component out of range")
    return parts


def fields(value: object, expected: Iterable[str]) -> None:
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ValueError("invalid inventory fields")


def prior_targets(prior: object) -> dict[str, rc.FeedTarget]:
    fields(prior, ("schema", "channel", "targets"))
    prior = cast("rc.Feed", prior)
    if type(prior["schema"]) is not int or prior["schema"] != 1:
        raise ValueError("invalid prior schema")
    if prior["channel"] != "stable" or not isinstance(prior["targets"], dict):
        raise ValueError("invalid prior channel or targets")
    result = {}
    for target, entry in prior["targets"].items():
        if target not in TARGETS:
            raise ValueError("unknown prior target")
        fields(entry, ("version", "tag", "method"))
        tag = entry["tag"]
        version(tag)
        if entry["version"] != tag[1:] or entry["method"] not in (
            "package",
            "store",
            "build-from-source",
        ):
            raise ValueError("invalid prior target entry")
        result[target] = entry.copy()
    return result


def expected_package(target: str, tag: str) -> str | None:
    release = tag[1:]
    return {
        "linux-x86_64": f"dyapp_{release}_amd64.deb",
        "windows-x86_64": f"dyapp-{release}-x86_64-windows.zip",
    }.get(target)


def validated_assets(release: rc.Release) -> dict[str, tuple[str, str]]:
    if not isinstance(release["assets"], list) or len(release["assets"]) > 100:
        raise ValueError("invalid asset inventory")
    assets = {}
    for asset in release["assets"]:
        fields(asset, ("name", "state", "sha256"))
        name, digest = asset["name"], asset["sha256"]
        if not isinstance(name, str) or not name or len(name) > 160:
            raise ValueError("invalid or duplicate asset")
        if not isinstance(digest, str) or not DIGEST.fullmatch(digest):
            raise ValueError("invalid or duplicate asset")
        if name in assets:
            raise ValueError("invalid or duplicate asset")
        assets[name] = (asset["state"], digest)
    return assets


def validated_attestations(
    release: rc.Release, assets: Mapping[str, tuple[str, str]]
) -> set[tuple[str, str]]:
    if not isinstance(release["attestations"], list):
        raise ValueError("invalid attestation inventory")
    if len(release["attestations"]) > 100:
        raise ValueError("invalid attestation inventory")
    attestations = set()
    for item in release["attestations"]:
        fields(item, ("name", "sha256"))
        pair = (item["name"], item["sha256"])
        if not isinstance(pair[0], str) or pair[0] not in assets:
            raise ValueError("invalid or duplicate attestation")
        if not isinstance(pair[1], str) or not DIGEST.fullmatch(pair[1]):
            raise ValueError("invalid or duplicate attestation")
        if pair in attestations:
            raise ValueError("invalid or duplicate attestation")
        attestations.add(pair)
    return attestations


def validated_source_builds(release: rc.Release) -> set[tuple[str, str]]:
    if not isinstance(release["source_builds"], list):
        raise ValueError("invalid source build inventory")
    if len(release["source_builds"]) > 5:
        raise ValueError("invalid source build inventory")
    source_builds = set()
    for item in release["source_builds"]:
        fields(item, ("target", "commit"))
        pair = (item["target"], item["commit"])
        if not isinstance(pair[0], str) or pair[0] not in SOURCE_TARGETS:
            raise ValueError("invalid or duplicate source build")
        if not isinstance(pair[1], str) or not COMMIT.fullmatch(pair[1]):
            raise ValueError("invalid or duplicate source build")
        if pair in source_builds:
            raise ValueError("invalid or duplicate source build")
        source_builds.add(pair)
    return source_builds


def verified_claim(
    target: str,
    claim: object,
    tag: str,
    commit: str,
    assets: Mapping[str, tuple[str, str]],
    attestations: Container[tuple[str, str]],
    source_builds: Container[tuple[str, str]],
) -> rc.FeedTarget:
    if target not in TARGETS or not isinstance(claim, dict):
        raise ValueError("unknown client target")
    method = claim.get("method")
    if method == "package":
        fields(claim, ("method", "asset"))
        expected = expected_package(target, tag)
        if expected is None or claim["asset"] != expected:
            raise ValueError("unexpected package asset")
        state_digest = assets.get(expected)
        if state_digest is None or state_digest[0] != "uploaded":
            raise ValueError("package is missing or not attested")
        if (expected, state_digest[1]) not in attestations:
            raise ValueError("package is missing or not attested")
    elif method == "build-from-source":
        fields(claim, ("method",))
        if target not in SOURCE_TARGETS:
            raise ValueError("source build not verified")
        if (target, commit) not in source_builds:
            raise ValueError("source build not verified")
    else:
        # No pinned store listing exists yet; reject store claims too.
        raise ValueError("unsupported update method")
    return {"version": tag[1:], "tag": tag, "method": method}


def validated_release(release: object) -> dict[str, rc.FeedTarget]:
    fields(
        release,
        (
            "tag",
            "commit",
            "draft",
            "prerelease",
            "assets",
            "attestations",
            "source_builds",
            "targets",
        ),
    )
    release = cast("rc.Release", release)
    tag = release["tag"]
    version(tag)
    commit = release["commit"]
    if not isinstance(commit, str) or not COMMIT.fullmatch(commit):
        raise ValueError("invalid release commit")
    if type(release["draft"]) is not bool:
        raise ValueError("invalid release publication state")
    if type(release["prerelease"]) is not bool:
        raise ValueError("invalid release publication state")
    assets = validated_assets(release)
    attestations = validated_attestations(release, assets)
    source_builds = validated_source_builds(release)
    targets = release["targets"]
    if not isinstance(targets, dict) or len(targets) > len(TARGETS):
        raise ValueError("invalid target inventory")
    if (release["draft"] or release["prerelease"]) and targets:
        raise ValueError("unpublished release advertises clients")
    result = {}
    for target, claim in targets.items():
        result[target] = verified_claim(
            target, claim, tag, commit, assets, attestations, source_builds
        )
    return result


def merge_target_claims(
    ordered: Sequence[tuple[tuple[int, ...], dict[str, rc.FeedTarget]]],
    targets: dict[str, rc.FeedTarget],
) -> None:
    for _, claims in sorted(ordered):
        for target, entry in claims.items():
            previous = targets.get(target)
            if previous:
                if version(entry["tag"]) < version(previous["tag"]):
                    # An older release in a full inventory is expected.
                    continue
                if entry["tag"] == previous["tag"] and entry != previous:
                    raise ValueError(
                        "published target changed without version bump"
                    )
            targets[target] = entry


def build(inventory: object, prior: object = None) -> rc.Feed:
    fields(inventory, ("schema", "releases"))
    inventory = cast("rc.Inventory", inventory)
    if type(inventory["schema"]) is not int or inventory["schema"] != 1:
        raise ValueError("invalid inventory schema")
    releases = inventory["releases"]
    if not isinstance(releases, list) or len(releases) > 100:
        raise ValueError("invalid release inventory")
    empty = {"schema": 1, "channel": "stable", "targets": {}}
    targets = prior_targets(empty if prior is None else prior)
    seen = set()
    ordered = []
    for release in releases:
        claims = validated_release(release)
        tag = release["tag"]
        if tag in seen:
            raise ValueError("duplicate release tag")
        seen.add(tag)
        if not release["draft"] and not release["prerelease"]:
            ordered.append((version(tag), claims))
    merge_target_claims(ordered, targets)
    for target in targets:
        candidates = [entry[target] for _, entry in ordered if target in entry]
        newest = max(
            (version(item["tag"]) for item in candidates), default=None
        )
        if newest is not None and newest < version(targets[target]["tag"]):
            raise ValueError("target rollback")
    return {"schema": 1, "channel": "stable", "targets": targets}


def main() -> "int":
    if len(sys.argv) not in (2, 3):
        print(__doc__, file=sys.stderr)
        return 2
    try:
        inventory = read_json(sys.argv[1], 1024 * 1024)
        previous = None
        if len(sys.argv) == 3:
            previous = read_json(sys.argv[2], 64 * 1024)
        result = build(inventory, previous)
    except (OSError, ValueError, TypeError, KeyError) as error:
        print(f"update feed rejected: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
