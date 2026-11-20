"""API37 reuses artifacts with isolated device state and visible failures."""

from ipaddress import ip_address, ip_network
import json
from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]


def mounts(volumes: list[str]) -> dict[str, list[str]]:
    pairs = (item.split(":") for item in volumes)
    return {parts[1]: parts for parts in pairs}


class AndroidPermissionCi(unittest.TestCase):
    def setUp(self) -> None:
        self.config = yaml.safe_load(
            (ROOT / "tests/docker/compose.ui.yml").read_text()
        )
        self.services = self.config["services"]

    def test_platforms_share_only_the_built_artifacts(self) -> None:
        legacy = self.services["android-emulator"]
        modern = self.services["android-permission-emulator"]
        old, new = mounts(legacy["volumes"]), mounts(modern["volumes"])
        for target in ("/avd", "/reports"):
            self.assertNotEqual(old[target][0], new[target][0])
        self.assertEqual(old["/app/build"][0], new["/app/build"][0])
        self.assertEqual(new["/app/build"][-1], "ro")
        self.assertEqual(legacy["healthcheck"], modern["healthcheck"])
        image = modern["environment"]["DYAPP_EMULATOR_IMAGE"]
        self.assertTrue(image.startswith("system-images;android-37."))
        self.assertTrue(self.config["networks"]["android-ui"]["internal"])

    def test_non_lan_control_is_owned_internal_and_unprivileged(self) -> None:
        modern = self.services["android-permission-emulator"]
        name = "android-permission-public"
        address = modern["networks"][name]["ipv4_address"]
        config = self.config["networks"][name]
        subnet = ip_network(config["ipam"]["config"][0]["subnet"])
        self.assertEqual(str(subnet), "198.51.100.0/24")
        self.assertIn(ip_address(address), subnet)
        self.assertTrue(config["internal"])
        self.assertEqual(modern["cap_drop"], ["ALL"])
        self.assertNotIn("cap_add", modern)
        self.assertNotIn("ports", modern)
        self.assertEqual(set(modern["networks"]), {"android-ui", name})
        self.assertNotIn(name, self.services["android-emulator"]["networks"])
        self.assertNotIn(
            name, self.services["android-permission-test"]["networks"]
        )
        self.assertEqual(
            set(json.loads(modern["environment"]["DYAPPD_LISTEN"])),
            {"127.0.0.1:33445", f"{address}:33445"},
        )
        runner = (
            ROOT / "tests/scripts/android-test-in-container.sh"
        ).read_text()
        self.assertIn(f'"{address}:33445"', runner)

    def test_reports_and_transport_follow_the_owned_device(
        self,
    ) -> None:
        service = self.services["android-permission-test"]
        dependency = next(iter(service["depends_on"]))
        self.assertEqual(
            service["depends_on"][dependency]["condition"], "service_healthy"
        )
        self.assertEqual(
            service["environment"]["ADB_SERVER_SOCKET"],
            f"tcp:{dependency}:5037",
        )
        self.assertEqual(
            mounts(service["volumes"])["/reports"],
            mounts(self.services[dependency]["volumes"])["/reports"],
        )
        self.assertEqual(service["command"][-1], "permissions")

    def test_both_devices_receive_the_same_kvm_access(self) -> None:
        overlay = yaml.safe_load(
            (ROOT / "tests/docker/compose.kvm.yml").read_text()
        )
        services = overlay["services"]
        self.assertEqual(
            services["android-emulator"],
            services["android-permission-emulator"],
        )

    def test_emulator_lanes_share_packages_but_run_in_parallel(self) -> None:
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/ci.yml").read_text()
        )
        lanes = workflow["jobs"]["docker-emulators"]
        self.assertEqual(lanes["needs"], "docker")
        self.assertFalse(lanes["strategy"]["fail-fast"])
        matrix = lanes["strategy"]["matrix"]["include"]
        self.assertEqual(
            {item["lane"] for item in matrix},
            {"android-emulator", "android-permissions"},
        )
        self.assertEqual(
            {item["budget"] for item in matrix},
            {15, 30},
        )
        self.assertIn("docker-emulators", workflow["jobs"]["summary"]["needs"])
        build = workflow["jobs"]["docker"]["steps"]
        packages = next(
            step
            for step in build
            if step.get("with", {}).get("name") == "android-runtime-packages"
        )
        self.assertIn(
            "build/android-build/debug/dyappd", packages["with"]["path"]
        )
        self.assertIn("app-debug-androidTest.apk", packages["with"]["path"])
        for lane in lanes["steps"]:
            if lane.get("name") == "test":
                self.assertEqual(lane["env"]["DYAPP_ANDROID_SKIP_BUILD"], "1")


if __name__ == "__main__":
    unittest.main()
