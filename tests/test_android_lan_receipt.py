"""Route receipts cannot turn an old or incomplete device run green."""

import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "android_lan_receipt", ROOT / "tests/scripts/android_lan_receipt.py"
)
assert SPEC is not None and SPEC.loader is not None
RECEIPT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RECEIPT)


class AndroidLanReceipt(unittest.TestCase):
    def proof(self) -> dict[str, object]:
        return {
            "phase": "denied",
            "node": "10.0.2.2:33445",
            "publicNode": "198.51.100.2:33445",
            "nodePeer": "owned-node",
            "owner": "retained-owner",
            "process": "fresh-process",
            "sdk": 37,
            "target": 37,
        }

    def validate(self, value: object) -> None:
        RECEIPT.validate(
            value, "denied", "10.0.2.2:33445", "198.51.100.2:33445"
        )

    def test_complete_actual_phase_schema_is_accepted(self) -> None:
        self.validate(self.proof())

    def test_old_route_phase_and_incomplete_identity_are_refused(self) -> None:
        for key in self.proof():
            with self.subTest(missing=key):
                value = self.proof()
                del value[key]
                with self.assertRaises(ValueError):
                    self.validate(value)
        for key in ("phase", "node", "publicNode"):
            with self.subTest(replaced=key):
                value = self.proof()
                value[key] = "stale"
                with self.assertRaises(ValueError):
                    self.validate(value)

    def test_legacy_device_and_malformed_values_are_refused(self) -> None:
        for key in ("sdk", "target"):
            for wrong in (31, "37", None):
                with self.subTest(key=key, value=wrong):
                    value = self.proof()
                    value[key] = wrong
                    with self.assertRaises(ValueError):
                        self.validate(value)
        for malformed in (None, ["not a receipt"], "not a receipt"):
            with self.assertRaises(ValueError):
                self.validate(malformed)


if __name__ == "__main__":
    unittest.main()
