"""check-advisory-ignores.py: review dates on security advisory ignores."""
from datetime import date
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_advisory_ignores", ROOT / "tests/scripts/check-advisory-ignores.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
TODAY = date(2026, 10, 30)
AUDIT = '[advisories]\nignore = ["A"]\n'
OSV = '[[IgnoredVulns]]\nid = "B"\nignoreUntil = {}\n'


def deny(reason):
    return f'[advisories]\nignore = [{{ id = "A", reason = "{reason}" }}]\n'


def errors(reason="x; review by 2026-11-21 (issue #22)", until="2026-11-21",
           audit=AUDIT, expiry=False):
    return CHECK.errors(deny(reason), audit, {"osv": OSV.format(until)},
                        TODAY, expiry)


class AdvisoryIgnores(unittest.TestCase):
    def test_clean(self):
        self.assertEqual(errors(), [])

    def test_format(self):
        self.assertEqual(errors(reason="no date"),
                         ["deny.toml: A: no review date"])
        self.assertEqual(errors(reason="review by 2026-13-01 (issue #1)"),
                         ["deny.toml: A: no review date"])
        self.assertEqual(errors(reason="review by 2026-11-21"),
                         ["deny.toml: A: no review date"])
        self.assertIn("more than 90 days", errors(until="2027-02-01")[0])
        osv = CHECK.errors(deny("review by 2026-11-21 (issue #2)"), AUDIT,
                           {"osv": '[[IgnoredVulns]]\nid = "B"\n'}, TODAY)
        self.assertEqual(osv, ["osv: B: no review date"])

    def test_audit_ids(self):
        self.assertIn("['A', 'C']", errors(
            audit='[advisories]\nignore = ["C"]\n')[0])

    def test_expiry(self):
        past = "review by 2026-10-01 (issue #3)"
        self.assertEqual(errors(reason=past, until="2026-11-12"), [])
        found = errors(reason=past, until="2026-11-12", expiry=True)
        self.assertEqual(len(found), 2)
        self.assertTrue(all("is due" in line for line in found))
        self.assertEqual(errors(until="2026-11-13", expiry=True), [])


if __name__ == "__main__":
    unittest.main()
