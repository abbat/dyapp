"""Only verified CI-owned keyring readiness admits a desktop smoke case."""

import os
from pathlib import Path
import runpy
import subprocess
from unittest import TestCase, main, mock

NATIVE = runpy.run_path(
    str(Path(__file__).parent / "scripts/desktop_smoke_native.py")
)


def result(state: str, code: int = 0) -> subprocess.CompletedProcess[str]:
    return subprocess.CompletedProcess(
        [], code, state, "controlled probe error"
    )


class KeyringReadiness(TestCase):
    def test_unowned_provider_is_never_probed_or_replaced(self) -> None:
        with (
            mock.patch.dict(os.environ, {}, clear=True),
            mock.patch("subprocess.run") as run,
        ):
            NATIVE["unlock_keyring"]()
            run.assert_not_called()

    def test_healthy_provider_is_only_read(self) -> None:
        with (
            mock.patch.dict(os.environ, DYAPP_TEST_SECRET_SERVICE="1"),
            mock.patch(
                "subprocess.run", return_value=result("b false")
            ) as run,
        ):
            NATIVE["unlock_keyring"]()
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.args[0][0], "busctl")

    def test_failed_or_unknown_probe_cannot_admit_ui(self) -> None:
        for response in (
            result("", 1),
            result("unknown"),
            result("b true", 1),
        ):
            with (
                self.subTest(response=response),
                mock.patch.dict(os.environ, DYAPP_TEST_SECRET_SERVICE="1"),
                mock.patch("subprocess.run", return_value=response) as run,
            ):
                with self.assertRaisesRegex(
                    RuntimeError, "readiness probe failed"
                ):
                    NATIVE["unlock_keyring"]()
                self.assertEqual(run.call_count, 1)

    def test_locked_provider_requires_successful_replacement_and_readback(
        self,
    ) -> None:
        with (
            mock.patch.dict(os.environ, DYAPP_TEST_SECRET_SERVICE="1"),
            mock.patch(
                "subprocess.run",
                side_effect=[result("b true"), result(""), result("b false")],
            ) as run,
        ):
            NATIVE["unlock_keyring"]()
            self.assertEqual(run.call_count, 3)
            self.assertTrue(run.call_args_list[1].kwargs["check"])
        with (
            mock.patch.dict(os.environ, DYAPP_TEST_SECRET_SERVICE="1"),
            mock.patch(
                "subprocess.run",
                side_effect=[result("b true"), result(""), result("b true")],
            ),
        ):
            with self.assertRaisesRegex(RuntimeError, "remains locked"):
                NATIVE["unlock_keyring"]()

    def test_replacement_process_failure_is_not_ignored(self) -> None:
        with (
            mock.patch.dict(os.environ, DYAPP_TEST_SECRET_SERVICE="1"),
            mock.patch(
                "subprocess.run",
                side_effect=[
                    result("b true"),
                    subprocess.CalledProcessError(1, ["gnome-keyring-daemon"]),
                ],
            ),
        ):
            with self.assertRaises(subprocess.CalledProcessError):
                NATIVE["unlock_keyring"]()


if __name__ == "__main__":
    main()
