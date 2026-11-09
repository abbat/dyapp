"""Tag feed staging preserves prior targets and rejects feed fetch failures."""

import typing
import urllib.request

import importlib.util
import json
from pathlib import Path
import unittest
from urllib.error import HTTPError
from email.message import Message

if typing.TYPE_CHECKING:
    import fixture_contracts as fc


SCRIPT = Path(__file__).parents[1] / "tools/scripts/stage-update-feed.py"
SPEC = importlib.util.spec_from_file_location("stage_update_feed", SCRIPT)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
STAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(STAGE)
DIGEST = "b" * 64
COMMIT = "a" * 40


def inventory(tag: "str", platform: "str | None" = None) -> "fc.Inventory":
    assets: list["fc.Asset"] = []
    attestations: list["fc.Attestation"] = []
    targets: dict[str, "fc.TargetClaim"] = {}
    if platform:
        name = f"dyapp_{tag[1:]}_amd64.deb"
        assets.append({"name": name, "state": "uploaded", "sha256": DIGEST})
        attestations.append({"name": name, "sha256": DIGEST})
        targets[platform] = {"method": "package", "asset": name}
    return {
        "schema": 1,
        "releases": [
            {
                "tag": tag,
                "commit": COMMIT,
                "draft": False,
                "prerelease": False,
                "assets": assets,
                "attestations": attestations,
                "source_builds": [],
                "targets": targets,
            }
        ],
    }


class Response:
    def __init__(self, data: "bytes", url: "str" = STAGE.FEED_URL) -> None:
        self.data = data
        self.url = url

    def __enter__(self) -> "Response":
        return self

    def __exit__(self, *_: "object") -> "typing.Literal[False]":
        return False

    def geturl(self) -> "str":
        return self.url

    def read(self, _: "int") -> "bytes":
        return self.data


class Opener:
    def __init__(self, response: "Response | Exception") -> None:
        self.response = response

    def open(
        self, request: "urllib.request.Request", timeout: "float"
    ) -> "Response":
        assert request.full_url == STAGE.FEED_URL
        assert timeout == 15
        if isinstance(self.response, Exception):
            raise self.response
        return self.response


def missing() -> "HTTPError":
    return HTTPError(STAGE.FEED_URL, 404, "not found", Message(), None)


class StageUpdateFeed(unittest.TestCase):
    def test_first_release_may_start_empty_feed(self) -> None:
        feed = STAGE.stage(
            "v0.0.1",
            Opener(missing()),
            inventory_fn=lambda tag: inventory(tag, "linux-x86_64"),
        )
        self.assertEqual(feed["targets"]["linux-x86_64"]["tag"], "v0.0.1")

    def test_server_only_release_preserves_prior_target(self) -> None:
        prior = STAGE.BUILD.build(inventory("v0.0.1", "linux-x86_64"))
        response = Response(json.dumps(prior).encode())
        feed = STAGE.stage("v0.0.2", Opener(response), inventory_fn=inventory)
        self.assertEqual(feed, prior)

    def test_prior_fetch_and_validation_fail_closed(self) -> None:
        prior = STAGE.BUILD.build(inventory("v0.0.1", "linux-x86_64"))
        failures = (
            Opener(missing()),
            Opener(Response(b"{}")),
            Opener(
                Response(
                    json.dumps(prior).encode(),
                    "https://other.example/stable.json",
                )
            ),
            Opener(Response(b"x" * (STAGE.MAX_PRIOR_BYTES + 1))),
        )
        for opener in failures:
            with self.subTest(opener=opener), self.assertRaises(ValueError):
                STAGE.stage("v0.0.2", opener, inventory_fn=inventory)

    def test_rollback_is_rejected(self) -> None:
        prior = STAGE.BUILD.build(inventory("v0.0.2", "linux-x86_64"))
        response = Response(json.dumps(prior).encode())
        with self.assertRaisesRegex(ValueError, "rollback"):
            STAGE.stage(
                "v0.0.1",
                Opener(response),
                inventory_fn=lambda tag: inventory(tag, "linux-x86_64"),
            )


if __name__ == "__main__":
    unittest.main()
