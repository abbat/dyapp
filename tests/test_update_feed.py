"""Offline stable feed publication rejects unverified client availability."""

import importlib.util
import json
from pathlib import Path
import unittest


SCRIPT = Path(__file__).parents[1] / "tools/scripts/build-update-feed.py"
SPEC = importlib.util.spec_from_file_location("build_update_feed", SCRIPT)
FEED = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FEED)
COMMIT = "a" * 40
DIGEST = "b" * 64


def release(tag="v0.0.1", targets=None):
    return {
        "tag": tag, "commit": COMMIT, "draft": False, "prerelease": False,
        "assets": [], "attestations": [], "source_builds": [],
        "targets": targets or {},
    }


def package(target, tag="v0.0.1"):
    name = FEED.expected_package(target, tag)
    item = release(tag, {target: {"method": "package", "asset": name}})
    item["assets"] = [{"name": name, "state": "uploaded", "sha256": DIGEST}]
    item["attestations"] = [{"name": name, "sha256": DIGEST}]
    return item


def inventory(*releases):
    return {"schema": 1, "releases": list(releases)}


def previous(**targets):
    return {"schema": 1, "channel": "stable", "targets": targets}


class UpdateFeed(unittest.TestCase):
    def test_server_release_keeps_last_verified_platform(self):
        prior = FEED.build(inventory(package("windows-x86_64")))
        actual = FEED.build(inventory(release("v0.0.2")), prior)
        self.assertEqual(actual, prior)
        self.assertEqual(FEED.build(inventory(release())), previous())

    def test_out_of_order_versions_advance_only_shipped_platform(self):
        old = package("windows-x86_64", "v0.0.9")
        newer = package("windows-x86_64", "v0.0.10")
        linux = package("linux-x86_64", "v0.0.10")
        newer["targets"].update(linux["targets"])
        newer["assets"].extend(linux["assets"])
        newer["attestations"].extend(linux["attestations"])
        actual = FEED.build(inventory(newer, old))
        self.assertEqual(actual["targets"]["windows-x86_64"]["tag"], "v0.0.10")
        self.assertEqual(actual["targets"]["linux-x86_64"]["tag"], "v0.0.10")

    def test_draft_and_prerelease_cannot_advertise_clients(self):
        for state in ("draft", "prerelease"):
            item = package("windows-x86_64")
            item[state] = True
            with self.subTest(state=state), self.assertRaises(ValueError):
                FEED.build(inventory(item))

    def test_package_requires_exact_uploaded_attested_asset(self):
        changes = (
            "missing", "failed", "unattested", "wrong-digest", "wrong-name",
        )
        for change in changes:
            item = package("windows-x86_64")
            if change == "missing":
                item["assets"] = []
            elif change == "failed":
                item["assets"][0]["state"] = "failed"
            elif change == "unattested":
                item["attestations"] = []
            elif change == "wrong-digest":
                item["attestations"][0]["sha256"] = "c" * 64
            else:
                item["targets"]["windows-x86_64"]["asset"] = "dyappd.deb"
            with self.subTest(change=change), self.assertRaises(ValueError):
                FEED.build(inventory(item))

    def test_source_build_requires_matching_commit_and_supported_target(self):
        item = release(targets={"ios": {"method": "build-from-source"}})
        item["source_builds"] = [{"target": "ios", "commit": COMMIT}]
        actual = FEED.build(inventory(item))["targets"]["ios"]["method"]
        self.assertEqual(actual, "build-from-source")
        item["source_builds"][0]["commit"] = "c" * 40
        with self.assertRaises(ValueError):
            FEED.build(inventory(item))
        item["targets"] = {"android": {"method": "build-from-source"}}
        item["source_builds"] = [{"target": "android", "commit": COMMIT}]
        with self.assertRaises(ValueError):
            FEED.build(inventory(item))

    def test_rollback_and_same_version_rewrite_fail(self):
        prior = FEED.build(inventory(package("windows-x86_64", "v0.0.2")))
        with self.assertRaisesRegex(ValueError, "rollback"):
            FEED.build(inventory(package("windows-x86_64", "v0.0.1")), prior)
        store_prior = previous(**{
            "windows-x86_64": {
                "version": "0.0.2", "tag": "v0.0.2", "method": "store"
            }
        })
        message = "changed without version bump"
        with self.assertRaisesRegex(ValueError, message):
            FEED.build(inventory(package("windows-x86_64", "v0.0.2")),
                       store_prior)

    def test_bad_versions_methods_and_prior_are_rejected(self):
        for tag in ("v01.0.0", "v0.0.1-rc1", "v2147483648.0.0", "0.0.1"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                FEED.build(inventory(release(tag)))
        item = release(targets={"ios": {"method": "store"}})
        with self.assertRaisesRegex(ValueError, "unsupported update method"):
            FEED.build(inventory(item))
        with self.assertRaises(ValueError):
            FEED.build(inventory(), {})
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            json.loads('{"schema":1,"schema":1}',
                       object_pairs_hook=FEED.unique_object)


if __name__ == "__main__":
    unittest.main()
