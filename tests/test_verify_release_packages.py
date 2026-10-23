"""Only downloaded bytes with pinned provenance become feed inventory."""

import hashlib
import importlib.util
import json
from pathlib import Path
import unittest


SCRIPT = Path(__file__).parents[1] / "tools/scripts/verify-release-packages.py"
SPEC = importlib.util.spec_from_file_location(
    "verify_release_packages", SCRIPT)
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)
BUILD_SCRIPT = Path(__file__).parents[1] / "tools/scripts/build-update-feed.py"
BUILD_SPEC = importlib.util.spec_from_file_location(
    "build_update_feed", BUILD_SCRIPT)
BUILD = importlib.util.module_from_spec(BUILD_SPEC)
BUILD_SPEC.loader.exec_module(BUILD)
TAG = "v0.0.1"
COMMIT = "a" * 40
NAME = "dyapp_0.0.1_amd64.deb"
BYTES = b"tested Linux package"
DIGEST = hashlib.sha256(BYTES).hexdigest()


def fake_runner(payload=BYTES, attestation=None, head=COMMIT,
                state="uploaded"):
    release = {
        "tag_name": TAG, "draft": False, "prerelease": False,
        "published_at": "2026-10-23T00:00:00Z",
        "assets": [{
            "name": NAME, "digest": "sha256:" + DIGEST,
            "state": state, "size": len(BYTES),
        }],
    }
    if attestation is None:
        attestation = [{"verificationResult": {"statement": {}}}]
    calls = []

    def run(args, timeout=120):
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
    def test_verified_package_becomes_trusted_inventory(self):
        run, calls = fake_runner()
        result = VERIFY.inventory(TAG, run)
        item = result["releases"][0]
        self.assertEqual(item["targets"]["linux-x86_64"]["asset"], NAME)
        self.assertEqual(item["assets"][0]["sha256"], DIGEST)
        self.assertEqual(item["attestations"][0]["sha256"], DIGEST)
        self.assertEqual(BUILD.build(result)["targets"]["linux-x86_64"]["tag"],
                         TAG)
        verification = next(args for args in calls
                            if args[:3] == ["gh", "attestation", "verify"])
        for option, expected in (
            ("--repo", "abbat/dyapp"),
            ("--signer-workflow", VERIFY.SIGNER),
            ("--source-ref", "refs/tags/" + TAG),
            ("--source-digest", COMMIT),
        ):
            self.assertEqual(verification[verification.index(option) + 1],
                             expected)
        self.assertIn("--deny-self-hosted-runners", verification)

    def test_wrong_bytes_and_empty_attestation_fail(self):
        for kwargs in (
            {"payload": b"changed"},
            {"attestation": []},
            {"attestation": [{"verificationResult": {}}]},
        ):
            run, _ = fake_runner(**kwargs)
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                VERIFY.inventory(TAG, run)

    def test_tag_checkout_and_release_state_fail_closed(self):
        for kwargs in ({"head": "b" * 40}, {"state": "starter"}):
            run, _ = fake_runner(**kwargs)
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                VERIFY.inventory(TAG, run)

    def test_verifier_cli_failure_never_emits_inventory(self):
        run, _ = fake_runner()

        def failed(args, timeout=120):
            if args[:3] == ["gh", "attestation", "verify"]:
                raise ValueError("verification command failed: gh")
            return run(args, timeout=timeout)

        with self.assertRaisesRegex(ValueError, "verification command"):
            VERIFY.inventory(TAG, failed)


if __name__ == "__main__":
    unittest.main()
