"""Verify APK identity/signature with managed SDK tools, not a binary parser.

Call only after exact release bytes and workflow provenance have been verified.
The expected version and public signing-certificate pin come from trusted CI.
"""

from __future__ import annotations

from pathlib import Path
import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import release_contracts as rc

APP_ID = "com.dyapp"
DIGEST = re.compile(r"[0-9a-f]{64}\Z")
CODE = re.compile(r"[1-9][0-9]*\Z")
SIGNER = re.compile(
    r"Signer #([1-9][0-9]*) certificate SHA-256 digest: ([0-9a-f]{64})\Z"
)
MAX_CODE = 2**31 - 1


def signer_certificate(output: bytes, expected: str) -> str:
    if len(output) > 64 * 1024:
        raise ValueError("Android signer output exceeds limit")
    signers = []
    for line in output.decode("utf-8").splitlines():
        if "certificate SHA-256 digest:" not in line:
            continue
        match = SIGNER.fullmatch(line.strip())
        if match is None:
            raise ValueError("invalid Android signer metadata")
        signers.append(match.groups())
    if signers != [("1", expected)]:
        raise ValueError("Android signing certificate does not match pin")
    return expected


def metadata(
    apk: Path,
    expected_version: str,
    certificate: str,
    run: rc.CommandRunner,
) -> rc.AndroidPackage:
    if not DIGEST.fullmatch(certificate):
        raise ValueError("missing or invalid Android certificate pin")
    summary = run(["apkanalyzer", "apk", "summary", str(apk)])
    if len(summary) > 4096:
        raise ValueError("Android package metadata exceeds limit")
    fields = summary.decode("utf-8").split()
    if len(fields) != 3 or fields[0] != APP_ID:
        raise ValueError("unexpected Android application ID")
    if fields[2] != expected_version:
        raise ValueError("Android version does not match release")
    if not CODE.fullmatch(fields[1]) or int(fields[1]) > MAX_CODE:
        raise ValueError("invalid Android version code")
    verified = run(
        ["apksigner", "verify", "--verbose", "--print-certs", str(apk)]
    )
    return {
        "application_id": APP_ID,
        "version_name": fields[2],
        "version_code": int(fields[1]),
        "certificate_sha256": signer_certificate(verified, certificate),
    }


def verify_upgrade(
    current: rc.AndroidPackage, previous: rc.AndroidPackage
) -> None:
    if (
        current["application_id"] != APP_ID
        or previous["application_id"] != APP_ID
    ):
        raise ValueError("Android application ID changed")
    if current["certificate_sha256"] != previous["certificate_sha256"]:
        raise ValueError("Android signing certificate changed")
    if current["version_name"] == previous["version_name"]:
        if current != previous:
            raise ValueError(
                "Android publication changed without version bump"
            )
    elif current["version_code"] <= previous["version_code"]:
        raise ValueError("Android version code did not increase")
