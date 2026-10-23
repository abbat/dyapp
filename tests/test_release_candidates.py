"""GitHub release metadata alone yields untrusted package candidates."""

import importlib.util
import json
from pathlib import Path
import unittest


SCRIPT = Path(__file__).parents[1] / "tools/scripts/release-candidates.py"
SPEC = importlib.util.spec_from_file_location("release_candidates", SCRIPT)
CANDIDATES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CANDIDATES)
TAG = "v0.0.1"
COMMIT = "a" * 40
DIGEST = "b" * 64
LINUX = "dyapp_0.0.1_amd64.deb"
WINDOWS = "dyapp-0.0.1-x86_64-windows.zip"


def release(*assets):
    return {
        "tag_name": TAG, "draft": False, "prerelease": False,
        "published_at": "2026-10-23T00:00:00Z",
        "target_commitish": "main", "assets": list(assets),
    }


def asset(name, digest=DIGEST, state="uploaded", size=42):
    return {
        "name": name, "digest": f"sha256:{digest}",
        "state": state, "size": size,
    }


class ReleaseCandidates(unittest.TestCase):
    def test_server_only_release_has_no_client_claim(self):
        actual = CANDIDATES.candidates(
            release(asset("dyappd_0.0.1_amd64.deb")), TAG, COMMIT)
        self.assertEqual(actual["packages"], {})
        self.assertNotIn("releases", actual)
        self.assertNotIn("targets", actual)

    def test_exact_linux_and_windows_package_candidates(self):
        actual = CANDIDATES.candidates(
            release(asset(WINDOWS), asset(LINUX)), TAG, COMMIT)
        self.assertEqual(set(actual["packages"]),
                         {"linux-x86_64", "windows-x86_64"})
        self.assertEqual(actual["packages"]["linux-x86_64"]["sha256"],
                         DIGEST)
        self.assertEqual(actual["commit"], COMMIT)

    def test_release_identity_and_state_must_match(self):
        for change in ("tag_name", "draft", "prerelease", "published_at"):
            item = release(asset(LINUX))
            item[change] = {
                "tag_name": "v0.0.2", "draft": True,
                "prerelease": True, "published_at": None,
            }[change]
            with self.subTest(change=change), self.assertRaises(ValueError):
                CANDIDATES.candidates(item, TAG, COMMIT)
        for tag, commit in (("v01.0.1", COMMIT), (TAG, "bad")):
            with self.assertRaises(ValueError):
                CANDIDATES.candidates(release(), tag, commit)

    def test_bad_asset_metadata_is_rejected(self):
        variants = (
            asset(LINUX, state="starter"),
            asset(LINUX, digest="bad"),
            asset(LINUX, size=0),
            asset("dyapp_0.0.2_amd64.deb"),
            asset("dyapp-0.0.2-x86_64-windows.zip"),
        )
        for item in variants:
            with self.subTest(item=item), self.assertRaises(ValueError):
                CANDIDATES.candidates(release(item), TAG, COMMIT)
        with self.assertRaisesRegex(ValueError, "duplicate"):
            CANDIDATES.candidates(
                release(asset(LINUX), asset(LINUX)), TAG, COMMIT)

    def test_duplicate_json_keys_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "duplicate"):
            json.loads('{"tag_name":"v0.0.1","tag_name":"v0.0.1"}',
                       object_pairs_hook=CANDIDATES.unique_object)


if __name__ == "__main__":
    unittest.main()
