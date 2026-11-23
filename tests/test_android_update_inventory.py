"""Exercise candidate/provenance/SDK/feed admission.

External tools return controlled responses.
"""

from __future__ import annotations

from collections.abc import Sequence
import hashlib
import importlib.util
import json
from pathlib import Path
import typing
import unittest

if typing.TYPE_CHECKING:
    import release_contracts as rc

SCRIPT = Path(__file__).parents[1] / "tools/scripts/verify-release-packages.py"
SPEC = importlib.util.spec_from_file_location(
    "verify_android_inventory", SCRIPT
)
assert SPEC is not None and SPEC.loader is not None
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)
TAG = "v0.0.2"
OLD = "v0.0.1"
PIN = "c" * 64
EMPTY: rc.Feed = {"schema": 1, "channel": "stable", "targets": {}}


class AndroidRunner:
    def __init__(self) -> None:
        self.current_code = 2
        self.previous_code = 1
        self.current_pin = PIN
        self.previous_pin = PIN
        self.app_id = "com.dyapp"
        self.unsigned = False
        self.include_current = True
        self.fail_previous_proof = False
        self.calls: list[list[str]] = []

    def name(self, tag: str) -> str:
        return f"dyapp-{tag[1:]}-android.apk"

    def commit(self, tag: str) -> str:
        return ("b" if tag == TAG else "a") * 40

    def payload(self, tag: str) -> bytes:
        return ("controlled Android package " + tag).encode()

    def release(self, tag: str) -> bytes:
        assets = []
        if self.include_current or tag != TAG:
            payload = self.payload(tag)
            assets.append(
                {
                    "name": self.name(tag),
                    "state": "uploaded",
                    "size": len(payload),
                    "digest": "sha256:" + hashlib.sha256(payload).hexdigest(),
                }
            )
        return json.dumps(
            {
                "tag_name": tag,
                "draft": False,
                "prerelease": False,
                "published_at": "2026-11-01T00:00:00Z",
                "assets": assets,
            }
        ).encode()

    def __call__(self, args: Sequence[str], timeout: int = 120) -> bytes:
        self.calls.append(list(args))
        if args[:2] == ["git", "rev-parse"]:
            tag = (
                TAG
                if args[2] == "HEAD"
                else args[2].split("/")[-1].split("^")[0]
            )
            return self.commit(tag).encode()
        if args[:2] == ["gh", "api"]:
            return self.release(args[2].split("/")[-1])
        if args[:3] == ["gh", "release", "download"]:
            tag = args[3]
            (Path(args[args.index("-D") + 1]) / self.name(tag)).write_bytes(
                self.payload(tag)
            )
            return b""
        if args[:3] == ["gh", "attestation", "verify"]:
            tag = args[args.index("--source-ref") + 1].split("/")[-1]
            assert args[args.index("--source-digest") + 1] == self.commit(tag)
            assert "--deny-self-hosted-runners" in args
            result: list[dict[str, object]] = (
                []
                if tag == OLD and self.fail_previous_proof
                else [{"verificationResult": {"statement": {}}}]
            )
            return json.dumps(result).encode()
        return self.sdk(args)

    def sdk(self, args: Sequence[str]) -> bytes:
        path = Path(args[-1])
        tag = TAG if path.name == self.name(TAG) else OLD
        assert path.read_bytes() == self.payload(tag)
        if args[0] == "apkanalyzer":
            code = self.current_code if tag == TAG else self.previous_code
            return f"{self.app_id} {code} {tag[1:]}".encode()
        if args[0] == "apksigner":
            if self.unsigned:
                raise ValueError("controlled unsigned APK refusal")
            pin = self.current_pin if tag == TAG else self.previous_pin
            return f"Signer #1 certificate SHA-256 digest: {pin}".encode()
        raise AssertionError(args)


def previous(tag: str = OLD) -> rc.Feed:
    return {
        "schema": 1,
        "channel": "stable",
        "targets": {
            "android": {"version": tag[1:], "tag": tag, "method": "package"}
        },
    }


class AndroidUpdateInventoryTests(unittest.TestCase):
    def test_first_publication_and_upgrade_reach_the_actual_feed_builder(
        self,
    ) -> None:
        for prior in [EMPTY, previous()]:
            with self.subTest(prior=prior):
                runner = AndroidRunner()
                inventory = VERIFY.inventory(
                    TAG, runner, previous=prior, certificate=PIN
                )
                feed = VERIFY.BUILD.build(inventory, prior)
                self.assertEqual(
                    feed["targets"]["android"],
                    {
                        "version": "0.0.2",
                        "tag": TAG,
                        "method": "package",
                    },
                )
                proofs = [
                    call
                    for call in runner.calls
                    if call[:3] == ["gh", "attestation", "verify"]
                ]
                self.assertEqual(len(proofs), 2 if prior["targets"] else 1)

    def test_invalid_current_or_previous_package_cannot_be_advertised(
        self,
    ) -> None:
        for failure in [
            "app",
            "unsigned",
            "current-pin",
            "previous-pin",
            "equal-code",
            "lower-code",
            "prior-proof",
        ]:
            with self.subTest(failure=failure):
                runner = AndroidRunner()
                if failure == "app":
                    runner.app_id = "com.other"
                elif failure == "unsigned":
                    runner.unsigned = True
                elif failure == "current-pin":
                    runner.current_pin = "d" * 64
                elif failure == "previous-pin":
                    runner.previous_pin = "d" * 64
                elif failure == "equal-code":
                    runner.current_code = 1
                elif failure == "lower-code":
                    runner.previous_code = 3
                else:
                    runner.fail_previous_proof = True
                with self.assertRaises(ValueError):
                    VERIFY.inventory(
                        TAG, runner, previous=previous(), certificate=PIN
                    )

    def test_same_tag_replay_and_server_only_retention_are_supported(
        self,
    ) -> None:
        runner = AndroidRunner()
        prior = previous(TAG)
        inventory = VERIFY.inventory(
            TAG, runner, previous=prior, certificate=PIN
        )
        self.assertEqual(VERIFY.BUILD.build(inventory, prior), prior)
        self.assertEqual(
            sum(
                call[:3] == ["gh", "attestation", "verify"]
                for call in runner.calls
            ),
            1,
        )
        runner = AndroidRunner()
        runner.include_current = False
        inventory = VERIFY.inventory(
            TAG, runner, previous=previous(), certificate=""
        )
        self.assertEqual(VERIFY.BUILD.build(inventory, previous()), previous())
        self.assertFalse(
            any(
                call[0] in ["apkanalyzer", "apksigner"]
                for call in runner.calls
            )
        )
