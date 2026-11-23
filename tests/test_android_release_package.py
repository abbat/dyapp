"""Controlled SDK output verifies policy.

Actual APK cryptography stays with the SDK tools.
"""

from __future__ import annotations

from collections.abc import Sequence
import importlib.util
from pathlib import Path
import typing
import unittest

if typing.TYPE_CHECKING:
    import release_contracts as rc

SCRIPT = Path(__file__).parents[1] / "tools/scripts/android-release-package.py"
SPEC = importlib.util.spec_from_file_location(
    "android_release_package", SCRIPT
)
assert SPEC is not None and SPEC.loader is not None
_module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(_module)
SDK = typing.cast("rc.AndroidPackageModule", _module)
PIN = "a" * 64
APK = Path("candidate.apk")


def runner(
    summary: bytes = b"com.dyapp 2 0.0.2\n",
    signatures: bytes | None = None,
) -> tuple[rc.CommandRunner, list[tuple[str, ...]]]:
    calls: list[tuple[str, ...]] = []
    output = (
        signatures
        if signatures is not None
        else (
            f"Verifies\nSigner #1 certificate SHA-256 digest: {PIN}\n".encode()
        )
    )

    def run(args: Sequence[str], timeout: int = 120) -> bytes:
        calls.append(tuple(args))
        if args[0] == "apkanalyzer":
            return summary
        if args[0] == "apksigner":
            return output
        raise AssertionError(args)

    return run, calls


class AndroidReleasePackageTests(unittest.TestCase):
    def test_sdk_commands_bind_app_version_code_and_the_expected_certificate(
        self,
    ) -> None:
        run, calls = runner()
        actual = SDK.metadata(APK, "0.0.2", PIN, run)
        self.assertEqual(
            actual,
            {
                "application_id": "com.dyapp",
                "version_name": "0.0.2",
                "version_code": 2,
                "certificate_sha256": PIN,
            },
        )
        self.assertEqual(
            calls,
            [
                ("apkanalyzer", "apk", "summary", "candidate.apk"),
                (
                    "apksigner",
                    "verify",
                    "--verbose",
                    "--print-certs",
                    "candidate.apk",
                ),
            ],
        )

    def test_missing_pin_runs_no_sdk_command(self) -> None:
        for pin in ["", "A" * 64, "a" * 63, "a" * 65]:
            with self.subTest(pin=pin):
                run, calls = runner()
                with self.assertRaises(ValueError):
                    SDK.metadata(APK, "0.0.2", pin, run)
                self.assertEqual(calls, [])

    def test_unexpected_manifest_metadata_never_reaches_signature_acceptance(
        self,
    ) -> None:
        for summary in [
            b"",
            b"com.dyapp 2",
            b"com.dyapp 2 0.0.2 extra",
            b"com.dyapp.debug 2 0.0.2",
            b"com.dyapp 2 0.0.1",
            b"com.dyapp 0 0.0.2",
            b"com.dyapp -1 0.0.2",
            b"com.dyapp 02 0.0.2",
            b"com.dyapp 2147483648 0.0.2",
            "com.dyapp ٢ 0.0.2".encode(),
            b"x" * 4097,
        ]:
            with self.subTest(summary=summary[:80]):
                run, calls = runner(summary)
                with self.assertRaises(ValueError):
                    SDK.metadata(APK, "0.0.2", PIN, run)
                self.assertEqual(len(calls), 1)

    def test_missing_wrong_or_ambiguous_signers_are_rejected(self) -> None:
        valid = f"Signer #1 certificate SHA-256 digest: {PIN}\n"
        for output in [
            "",
            valid.replace(PIN, "b" * 64),
            valid + valid,
            valid + valid.replace("#1", "#2"),
            valid.replace("#1", "#2"),
            valid.replace(PIN, "malformed"),
            "x" * (64 * 1024 + 1),
        ]:
            with self.subTest(output=output[:100]):
                run, _ = runner(signatures=output.encode())
                with self.assertRaises(ValueError):
                    SDK.metadata(APK, "0.0.2", PIN, run)

    def test_a_failed_sdk_verification_is_not_a_metadata_success(self) -> None:
        def failed(args: Sequence[str], timeout: int = 120) -> bytes:
            if args[0] == "apksigner":
                raise ValueError("controlled SDK signature failure")
            return b"com.dyapp 2 0.0.2"

        with self.assertRaisesRegex(ValueError, "controlled SDK"):
            SDK.metadata(APK, "0.0.2", PIN, failed)

    def test_upgrade_advances_code_and_same_publication_is_idempotent(
        self,
    ) -> None:
        run, _ = runner()
        current = SDK.metadata(APK, "0.0.2", PIN, run)
        previous = current.copy()
        previous["version_name"] = "0.0.1"
        previous["version_code"] = 1
        SDK.verify_upgrade(current, previous)
        SDK.verify_upgrade(current, current.copy())
        wrong_app = current.copy()
        wrong_app["application_id"] = "com.other"
        wrong_cert = current.copy()
        wrong_cert["certificate_sha256"] = "b" * 64
        equal_code = current.copy()
        equal_code["version_name"] = "0.0.1"
        newer_code = equal_code.copy()
        newer_code["version_code"] = 3
        replaced = current.copy()
        replaced["version_code"] = 3
        for invalid in [
            wrong_app,
            wrong_cert,
            equal_code,
            newer_code,
            replaced,
        ]:
            with self.subTest(previous=invalid):
                with self.assertRaises(ValueError):
                    SDK.verify_upgrade(current, invalid)
