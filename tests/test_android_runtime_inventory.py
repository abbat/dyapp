"""Synthetic ZIP tests; no Android native behavior is executed."""

from hashlib import sha256
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import unittest
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tests/scripts/android_runtime_inventory.py"
SPEC = importlib.util.spec_from_file_location(
    "android_runtime_inventory", SCRIPT
)
assert SPEC is not None and SPEC.loader is not None
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class InventoryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.root = (
            ROOT / "build/android-runtime-inventory-tests" / uuid.uuid4().hex
        )
        self.root.mkdir(parents=True)
        self.native = b"prepared x86 JNI bytes"
        self.aar = b"prepared AAR checksum fixture"
        self.filename = "jna-5.19.1-hardened.aar"
        self.manifest = {
            "coordinate": "net.java.dev.jna:jna:5.19.1",
            "filename": "jna-5.19.1-hardened.aar",
            "sha256": sha256(self.aar).hexdigest(),
            "native_sha256": {
                "x86_64": sha256(self.native).hexdigest(),
                "arm64-v8a": sha256(b"prepared arm JNI bytes").hexdigest(),
            },
        }
        (self.root / self.filename).write_bytes(self.aar)
        self.save_manifest()
        self.entries = [
            ("classes.dex", b"fixture"),
            ("lib/x86_64/libjnidispatch.so", self.native),
            ("lib/x86_64/libdyapp_ffi.so", b"prepared FFI bytes"),
        ]

    def save_manifest(self) -> None:
        (self.root / "manifest.json").write_text(
            json.dumps(self.manifest) + "\n"
        )

    def apk(self, name: str, entries: list[tuple[str, bytes]]) -> Path:
        path = self.root / name
        with zipfile.ZipFile(path, "w") as archive:
            for filename, content in entries:
                archive.writestr(
                    zipfile.ZipInfo(filename, (1980, 1, 1, 0, 0, 0)),
                    content,
                )
        return path

    def test_records_exact_apk_and_all_native_bytes(self) -> None:
        prepared = CHECK.producer(self.root)
        path = self.apk(
            "app-base.apk",
            self.entries + [("lib/x86_64/libcodec.so", b"codec")],
        )
        actual = CHECK.inventory(path, prepared, application=True)
        self.assertEqual(
            actual["sha256"], sha256(path.read_bytes()).hexdigest()
        )
        self.assertEqual(actual["bytes"], path.stat().st_size)
        self.assertEqual(
            actual["native_sha256"],
            {
                name: sha256(content).hexdigest()
                for name, content in self.entries[1:]
                + [("lib/x86_64/libcodec.so", b"codec")]
            },
        )

    def test_test_package_does_not_need_ffi_but_checks_any_packaged_jna(
        self,
    ) -> None:
        prepared = CHECK.producer(self.root)
        path = self.apk("test.apk", [("classes.dex", b"tests")])
        self.assertEqual(
            CHECK.inventory(path, prepared, application=False)[
                "native_sha256"
            ],
            {},
        )
        bad = self.apk(
            "bad-test.apk", [("lib/x86_64/libjnidispatch.so", b"wrong")]
        )
        with self.assertRaisesRegex(
            ValueError, "Packaged JNA checksum mismatch"
        ):
            CHECK.inventory(bad, prepared, application=False)

    def test_wrong_jna_cannot_produce_inventory(self) -> None:
        path = self.apk(
            "wrong.apk",
            self.entries[:1]
            + [
                ("lib/x86_64/libjnidispatch.so", b"wrong"),
                self.entries[2],
            ],
        )
        with self.assertRaisesRegex(
            ValueError, "Packaged JNA checksum mismatch"
        ):
            CHECK.inventory(path, CHECK.producer(self.root), application=True)

    def test_each_required_application_library_is_checked(self) -> None:
        prepared = CHECK.producer(self.root)
        for missing in (1, 2):
            with self.subTest(missing=missing):
                path = self.apk(
                    f"missing-{missing}.apk",
                    [
                        row
                        for index, row in enumerate(self.entries)
                        if index != missing
                    ],
                )
                with self.assertRaisesRegex(
                    ValueError, "lacks x86_64 JNA or FFI"
                ):
                    CHECK.inventory(path, prepared, application=True)

    def test_every_additional_application_abi_needs_jna_and_ffi(self) -> None:
        prepared = CHECK.producer(self.root)
        fragments = [
            ("lib/arm64-v8a/libjnidispatch.so", b"prepared arm JNI bytes"),
            ("lib/arm64-v8a/libdyapp_ffi.so", b"arm FFI"),
            ("lib/arm64-v8a/libcodec.so", b"arm codec"),
        ]
        for index, fragment in enumerate(fragments):
            with self.subTest(fragment=fragment[0]):
                path = self.apk(
                    f"partial-arm-{index}.apk", self.entries + [fragment]
                )
                with self.assertRaisesRegex(
                    ValueError, "lacks arm64-v8a JNA or FFI"
                ):
                    CHECK.inventory(path, prepared, application=True)

    def test_complete_two_abi_application_records_both_payloads(self) -> None:
        arm = [
            ("lib/arm64-v8a/libjnidispatch.so", b"prepared arm JNI bytes"),
            ("lib/arm64-v8a/libdyapp_ffi.so", b"arm FFI"),
        ]
        path = self.apk("both-abis.apk", self.entries + arm)
        actual = CHECK.inventory(
            path, CHECK.producer(self.root), application=True
        )
        self.assertEqual(
            actual["native_sha256"],
            {
                name: sha256(content).hexdigest()
                for name, content in self.entries[1:] + arm
            },
        )

    def test_empty_ffi_is_not_a_present_native_payload(self) -> None:
        path = self.apk(
            "empty-ffi.apk",
            self.entries[:2] + [("lib/x86_64/libdyapp_ffi.so", b"")],
        )
        with self.assertRaisesRegex(ValueError, "Empty APK native payload"):
            CHECK.inventory(path, CHECK.producer(self.root), application=True)

    def test_duplicate_entries_are_refused(self) -> None:
        with self.assertWarnsRegex(UserWarning, "Duplicate name"):
            path = self.apk("duplicate.apk", self.entries + [self.entries[1]])
        with self.assertRaisesRegex(ValueError, "Duplicate APK entries"):
            CHECK.inventory(path, CHECK.producer(self.root), application=True)

    def test_unsupported_native_abi_is_refused(self) -> None:
        path = self.apk(
            "unsupported.apk", self.entries + [("lib/x86/libcodec.so", b"old")]
        )
        with self.assertRaisesRegex(
            ValueError, "Unsupported APK native entry"
        ):
            CHECK.inventory(path, CHECK.producer(self.root), application=True)

    def test_stale_prepared_aar_is_refused(self) -> None:
        (self.root / self.filename).write_bytes(b"changed after preparation")
        with self.assertRaisesRegex(
            ValueError, "Prepared JNA AAR checksum mismatch"
        ):
            CHECK.producer(self.root)

    def test_prepared_metadata_cannot_select_an_arbitrary_path(self) -> None:
        self.manifest["filename"] = "../other.aar"
        self.save_manifest()
        with self.assertRaisesRegex(
            ValueError, "Prepared JNA filename mismatch"
        ):
            CHECK.producer(self.root)

    def run_cli(
        self, wrong_jna: bool = False
    ) -> subprocess.CompletedProcess[str]:
        base = self.apk("app-base.apk", self.entries)
        upgrade = self.apk(
            "app-upgrade.apk",
            [("classes.dex", b"upgraded fixture"), *self.entries[1:]],
        )
        tests = self.apk("tests.apk", [("classes.dex", b"tests")])
        if wrong_jna:
            base = self.apk(
                "bad.apk",
                [
                    ("lib/x86_64/libjnidispatch.so", b"wrong"),
                    self.entries[2],
                ],
            )
        return subprocess.run(
            [
                sys.executable,
                "-B",
                str(SCRIPT),
                "--prepared",
                str(self.root),
                "--output",
                str(self.root / "receipt.json"),
                "--application",
                str(base),
                "--application",
                str(upgrade),
                "--test",
                str(tests),
            ],
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )

    def test_cli_records_the_complete_package_set(self) -> None:
        result = self.run_cli()
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads((self.root / "receipt.json").read_text())
        self.assertEqual(receipt["schema"], 1)
        self.assertEqual(receipt["producer"]["manifest"], self.manifest)
        self.assertEqual(
            [item["filename"] for item in receipt["applications"]],
            ["app-base.apk", "app-upgrade.apk"],
        )
        self.assertEqual(receipt["tests"][0]["filename"], "tests.apk")
        self.assertNotEqual(
            receipt["applications"][0]["sha256"],
            receipt["applications"][1]["sha256"],
        )

    def test_cli_refusal_leaves_no_success_receipt(self) -> None:
        result = self.run_cli(wrong_jna=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Packaged JNA checksum mismatch", result.stderr)
        self.assertFalse((self.root / "receipt.json").exists())

    def test_cli_never_overwrites_an_existing_receipt(self) -> None:
        receipt = self.root / "receipt.json"
        receipt.write_text("earlier receipt")
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(receipt.read_text(), "earlier receipt")

    def test_nonobject_manifest_is_refused(self) -> None:
        (self.root / "manifest.json").write_text("[]")
        with self.assertRaisesRegex(
            ValueError, "Invalid prepared JNA manifest"
        ):
            CHECK.producer(self.root)


if __name__ == "__main__":
    unittest.main()
