"""API37 reuses artifacts with isolated device state and visible failures."""

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


if __name__ == "__main__":
    unittest.main()
