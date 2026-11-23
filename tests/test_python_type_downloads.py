"""Exercise bounded setup transport recovery without network or real waits."""

from email.message import Message
import io
import ssl
import unittest
from unittest.mock import patch
from urllib.error import HTTPError, URLError

from test_python_types import INSTALL, artifact


class ResetDuringRead(io.BytesIO):
    def read(self, size: int | None = -1) -> bytes:
        raise ConnectionResetError("controlled interrupted body")


class PythonTypeDownloads(unittest.TestCase):
    def test_wrapped_reset_recovers_through_a_new_verified_download(
        self,
    ) -> None:
        body = b"hash-locked fixture"
        response = io.BytesIO(body)
        with patch.object(
            INSTALL,
            "urlopen",
            side_effect=[URLError(ConnectionResetError("reset")), response],
        ) as opened:
            self.assertEqual(INSTALL.download(artifact(body)), body)
        self.assertEqual(opened.call_count, 2)
        self.assertTrue(response.closed)
        for call in opened.call_args_list:
            self.assertEqual(call.args, (artifact(body)["url"],))
            self.assertEqual(call.kwargs, {"timeout": 60})

    def test_verified_recovery_on_the_final_allowed_attempt(self) -> None:
        body = b"complete"
        response = io.BytesIO(body)
        with patch.object(
            INSTALL,
            "urlopen",
            side_effect=[
                TimeoutError("first attempt"),
                URLError(ConnectionResetError("second attempt")),
                response,
            ],
        ) as opened:
            self.assertEqual(INSTALL.download(artifact(body)), body)
        self.assertEqual(opened.call_count, 3)
        self.assertTrue(response.closed)

    def test_mid_body_reset_closes_the_failed_response_before_retry(
        self,
    ) -> None:
        failed = ResetDuringRead()
        success = io.BytesIO(b"complete")
        with patch.object(
            INSTALL, "urlopen", side_effect=[failed, success]
        ) as opened:
            self.assertEqual(
                INSTALL.download(artifact(b"complete")), b"complete"
            )
        self.assertEqual(opened.call_count, 2)
        self.assertTrue(failed.closed)
        self.assertTrue(success.closed)

    def test_persistent_resets_and_timeouts_have_a_finite_attempt_budget(
        self,
    ) -> None:
        for error in (
            ConnectionResetError("reset"),
            TimeoutError("timeout"),
            URLError(TimeoutError("wrapped timeout")),
        ):
            with self.subTest(error=type(error).__name__):
                with patch.object(
                    INSTALL, "urlopen", side_effect=error
                ) as opened:
                    with self.assertRaises(type(error)):
                        INSTALL.download(artifact(b"fixture"))
                self.assertEqual(opened.call_count, 3)

    def test_only_transient_http_statuses_retry_and_close_the_error_body(
        self,
    ) -> None:
        for status in (408, 500, 502, 503, 504):
            with self.subTest(status=status):
                data = b"complete"
                body = io.BytesIO(b"temporary failure")
                error = HTTPError(
                    artifact(data)["url"], status, "temporary", Message(), body
                )
                success = io.BytesIO(data)
                with patch.object(
                    INSTALL, "urlopen", side_effect=[error, success]
                ) as opened:
                    self.assertEqual(INSTALL.download(artifact(data)), data)
                self.assertEqual(opened.call_count, 2)
                self.assertTrue(body.closed)
                self.assertTrue(success.closed)

    def test_permanent_http_and_throttling_errors_do_not_retry(self) -> None:
        for status in (400, 401, 403, 404, 410, 429, 501):
            with self.subTest(status=status):
                body = io.BytesIO(b"refused")
                error = HTTPError(
                    artifact(b"x")["url"], status, "refused", Message(), body
                )
                with patch.object(
                    INSTALL, "urlopen", side_effect=error
                ) as opened:
                    with self.assertRaises(HTTPError):
                        INSTALL.download(artifact(b"x"))
                self.assertEqual(opened.call_count, 1)
                self.assertTrue(body.closed)

    def test_certificate_and_other_url_errors_are_not_transient(self) -> None:
        for reason in (
            ssl.SSLCertVerificationError("untrusted"),
            "invalid endpoint",
        ):
            with self.subTest(reason=str(reason)):
                error = URLError(reason)
                with patch.object(
                    INSTALL, "urlopen", side_effect=error
                ) as opened:
                    with self.assertRaises(URLError):
                        INSTALL.download(artifact(b"x"))
                self.assertEqual(opened.call_count, 1)

    def test_wrong_hash_or_oversized_body_is_never_retried(self) -> None:
        for maximum, expected in ((32, b"expected"), (2, b"corrupted")):
            with self.subTest(maximum=maximum):
                response = io.BytesIO(b"corrupted")
                with (
                    patch.object(INSTALL, "MAX_DOWNLOAD", maximum),
                    patch.object(
                        INSTALL, "urlopen", return_value=response
                    ) as opened,
                ):
                    with self.assertRaises(ValueError):
                        INSTALL.download(artifact(expected))
                self.assertEqual(opened.call_count, 1)
                self.assertTrue(response.closed)

    def test_origin_rejection_happens_before_any_network_attempt(self) -> None:
        invalid = artifact(b"fixture")
        invalid["url"] = "https://example.invalid/fixture.whl"
        with patch.object(INSTALL, "urlopen") as opened:
            with self.assertRaisesRegex(ValueError, "official PyPI"):
                INSTALL.download(invalid)
        opened.assert_not_called()


if __name__ == "__main__":
    unittest.main()
