#!/usr/bin/env python3
"""Stage a monotonic client feed after tagged release publication.

Usage: stage-update-feed.py TAG > stable.json
Requires a checked-out tag and gh authentication. Does not deploy Pages.
"""

import importlib.util
import json
from pathlib import Path
import sys
import urllib.error
import urllib.request


ROOT = Path(__file__).resolve().parents[2]
FEED_URL = "https://abbat.github.io/dyapp/updates/v1/stable.json"
MAX_PRIOR_BYTES = 64 * 1024


def load_module(name, script):
    spec = importlib.util.spec_from_file_location(name, ROOT / script)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BUILD = load_module("build_update_feed", "tools/scripts/build-update-feed.py")
VERIFY = load_module(
    "verify_release_packages", "tools/scripts/verify-release-packages.py")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, url):
        return None


def prior_feed(tag, opener=None):
    if opener is None:
        opener = urllib.request.build_opener(NoRedirect())
    request = urllib.request.Request(
        FEED_URL, headers={"Accept": "application/json"})
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
    return json.loads(data, object_pairs_hook=BUILD.unique_object)


def stage(tag, opener=None, inventory_fn=VERIFY.inventory):
    inventory = inventory_fn(tag)
    previous = prior_feed(tag, opener)
    return BUILD.build(inventory, previous)


def main():
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        result = stage(sys.argv[1])
    except (OSError, ValueError, TypeError, KeyError,
            urllib.error.URLError) as error:
        print(f"update feed staging rejected: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
