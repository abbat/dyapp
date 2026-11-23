"""Only downloaded bytes with pinned provenance become feed inventory."""

import typing


from collections.abc import Sequence
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

if typing.TYPE_CHECKING:
    import fixture_contracts as fc


SCRIPT = Path(__file__).parents[1] / "tools/scripts/verify-release-packages.py"
SPEC = importlib.util.spec_from_file_location(
    "verify_release_packages", SCRIPT
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)
BUILD_SCRIPT = Path(__file__).parents[1] / "tools/scripts/build-update-feed.py"
BUILD_SPEC = importlib.util.spec_from_file_location(
    "build_update_feed", BUILD_SCRIPT
)
assert BUILD_SPEC is not None and BUILD_SPEC.loader is not None, (
    "Python source loader is unavailable"
)
BUILD = importlib.util.module_from_spec(BUILD_SPEC)
BUILD_SPEC.loader.exec_module(BUILD)
TAG = "v0.0.1"
COMMIT = "a" * 40
NAME = "dyapp_0.0.1_amd64.deb"
BYTES = b"tested Linux package"
DIGEST = hashlib.sha256(BYTES).hexdigest()


def fake_runner(
    payload: "bytes" = BYTES,
    attestation: "fc.JSONValue" = None,
    head: "str" = COMMIT,
    state: str = "uploaded",
) -> "tuple[fc.ByteRunner, list[list[str]]]":
    release = {
        "tag_name": TAG,
        "draft": False,
        "prerelease": False,
        "published_at": "2026-10-23T00:00:00Z",
        "assets": [
            {
                "name": NAME,
                "digest": "sha256:" + DIGEST,
                "state": state,
                "size": len(BYTES),
            }
        ],
    }
    if attestation is None:
        attestation = [{"verificationResult": {"statement": {}}}]
    calls: list[list[str]] = []

    def run(args: "list[str]", timeout: int = 120) -> "bytes":
        calls.append(args)
        if args[:2] == ["git", "rev-parse"]:
            return (head if args[2] == "HEAD" else COMMIT).encode()
        if args[:2] == ["gh", "api"]:
            return json.dumps(release).encode()
        if args[:3] == ["gh", "release", "download"]:
            directory = Path(args[args.index("-D") + 1])
            (directory / NAME).write_bytes(payload)
            return b""
        if args[:3] == ["gh", "attestation", "verify"]:
            return json.dumps(attestation).encode()
        raise AssertionError(args)

    return run, calls


class VerifyReleasePackages(unittest.TestCase):
    def test_android_requires_prior_context_pin_and_a_supported_baseline(
        self,
    ) -> None:
        pin = "c" * 64
        package = {
            "name": "dyapp-0.0.1-android.apk",
            "size": 1,
            "sha256": DIGEST,
        }
        prior = {"schema": 1, "channel": "stable", "targets": {}}
        calls: list[list[str]] = []

        def sdk(args: Sequence[str], timeout: int = 120) -> bytes:
            calls.append(list(args))
            if args[0] == "apkanalyzer":
                return b"com.dyapp 1 0.0.1"
            if args[0] == "apksigner":
                return f"Signer #1 certificate SHA-256 digest: {pin}".encode()
            raise AssertionError(args)

        with self.assertRaisesRegex(ValueError, "previous feed"):
            VERIFY.verify_android(
                package, TAG, Path("published"), None, pin, sdk
            )
        self.assertEqual(calls, [])
        with self.assertRaisesRegex(ValueError, "certificate pin"):
            VERIFY.verify_android(
                package, TAG, Path("published"), prior, "", sdk
            )
        self.assertEqual(calls, [])
        VERIFY.verify_android(package, TAG, Path("published"), prior, pin, sdk)
        self.assertEqual(len(calls), 2)
        for method in ["package", "store", "build-from-source"]:
            baseline = {
                "schema": 1,
                "channel": "stable",
                "targets": {
                    "android": {
                        "version": "0.0.1",
                        "tag": TAG,
                        "method": method,
                    }
                },
            }
            if method == "package":
                VERIFY.verify_android(
                    package, TAG, Path("published"), baseline, pin, sdk
                )
            else:
                with self.assertRaisesRegex(ValueError, "migration"):
                    VERIFY.verify_android(
                        package, TAG, Path("published"), baseline, pin, sdk
                    )

    def test_verified_package_becomes_trusted_inventory(self) -> None:
        run, calls = fake_runner()
        result = VERIFY.inventory(TAG, run)
        item = result["releases"][0]
        self.assertEqual(item["targets"]["linux-x86_64"]["asset"], NAME)
        self.assertEqual(item["assets"][0]["sha256"], DIGEST)
        self.assertEqual(item["attestations"][0]["sha256"], DIGEST)
        self.assertEqual(
            BUILD.build(result)["targets"]["linux-x86_64"]["tag"], TAG
        )
        verification = next(
            args
            for args in calls
            if args[:3] == ["gh", "attestation", "verify"]
        )
        for option, expected in (
            ("--repo", "abbat/dyapp"),
            ("--signer-workflow", VERIFY.SIGNER),
            ("--source-ref", "refs/tags/" + TAG),
            ("--source-digest", COMMIT),
        ):
            self.assertEqual(
                verification[verification.index(option) + 1], expected
            )
        self.assertIn("--deny-self-hosted-runners", verification)

    def test_wrong_bytes_and_empty_attestation_fail(self) -> None:
        cases: tuple[fc.RunnerOptions, ...] = (
            {"payload": b"changed"},
            {"attestation": []},
            {"attestation": [{"verificationResult": {}}]},
        )
        for kwargs in cases:
            run, _ = fake_runner(**kwargs)
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                VERIFY.inventory(TAG, run)

    def test_tag_checkout_and_release_state_fail_closed(self) -> None:
        cases: tuple[fc.RunnerOptions, ...] = (
            {"head": "b" * 40},
            {"state": "starter"},
        )
        for kwargs in cases:
            run, _ = fake_runner(**kwargs)
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                VERIFY.inventory(TAG, run)

    def test_verifier_cli_failure_never_emits_inventory(self) -> None:
        run, _ = fake_runner()

        def failed(args: "list[str]", timeout: int = 120) -> "bytes":
            if args[:3] == ["gh", "attestation", "verify"]:
                raise ValueError("verification command failed: gh")
            return run(args, timeout=timeout)

        with self.assertRaisesRegex(ValueError, "verification command"):
            VERIFY.inventory(TAG, failed)


if __name__ == "__main__":
    unittest.main()
