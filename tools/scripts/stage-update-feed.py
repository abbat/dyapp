#!/usr/bin/env python3
"""Stage a monotonic client feed after tagged release publication.

Usage: stage-update-feed.py TAG > stable.json
Requires a checked-out tag and gh authentication. Does not deploy Pages.
"""

from __future__ import annotations

from collections.abc import Callable
from email.message import Message

import types
import typing

import importlib.util
import json
from pathlib import Path
import sys
import urllib.error
import urllib.request


if typing.TYPE_CHECKING:
    import release_contracts as rc

ROOT = Path(__file__).resolve().parents[2]
FEED_URL = "https://abbat.github.io/dyapp/updates/v1/stable.json"
MAX_PRIOR_BYTES = 64 * 1024


def load_module(name: "str", script: "str") -> "types.ModuleType":
    spec = importlib.util.spec_from_file_location(name, ROOT / script)
    assert spec is not None and spec.loader is not None, (
        "Python source loader is unavailable"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BUILD = typing.cast(
    "rc.FeedModule",
    load_module("build_update_feed", "tools/scripts/build-update-feed.py"),
)
VERIFY = typing.cast(
    "rc.InventoryModule",
    load_module(
        "verify_release_packages", "tools/scripts/verify-release-packages.py"
    ),
)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(
        self,
        request: "urllib.request.Request",
        response: "typing.IO[bytes]",
        code: "int",
        message: "str",
        headers: Message,
        url: "str",
    ) -> None:
        return None


def prior_feed(tag: str, opener: rc.Opener | None = None) -> object:
    if opener is None:
        opener = urllib.request.build_opener(NoRedirect())
    request = urllib.request.Request(
        FEED_URL, headers={"Accept": "application/json"}
    )
    try:
        with opener.open(request, timeout=15) as response:
            if response.geturl() != FEED_URL:
                raise ValueError("prior feed changed origin or path")
            data = response.read(MAX_PRIOR_BYTES + 1)
    except urllib.error.HTTPError as error:
        if error.code == 404 and tag == "v0.0.1":
            return {"schema": 1, "channel": "stable", "targets": {}}
        raise ValueError("prior feed unavailable") from error
    if len(data) > MAX_PRIOR_BYTES:
        raise ValueError("prior feed exceeds size limit")
    value: object = json.loads(data, object_pairs_hook=BUILD.unique_object)
    return value


def stage(
    tag: str,
    opener: rc.Opener | None = None,
    inventory_fn: Callable[[str], rc.Inventory] = VERIFY.inventory,
) -> rc.Feed:
    inventory = inventory_fn(tag)
    previous = prior_feed(tag, opener)
    return BUILD.build(inventory, previous)


def main() -> "int":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        result = stage(sys.argv[1])
    except (
        OSError,
        ValueError,
        TypeError,
        KeyError,
        urllib.error.URLError,
    ) as error:
        print(f"update feed staging rejected: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
