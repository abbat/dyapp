"""Sticker WebP measurements retain metadata and resource deltas."""

import importlib.util
import json
from pathlib import Path
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "sticker_webp_measurement", ROOT / "tests/scripts/sticker-webp-measure.py"
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
MEASURE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MEASURE)


class StickerWebpMeasurements(unittest.TestCase):
    def test_metadata_rejects_malformed_or_incomplete_output(self) -> None:
        for text in ("not json", "[]", "{}"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                MEASURE.decode_metadata(text, 10)

    def test_changing_wall_time_does_not_change_fixture_identity(self) -> None:
        baseline = MEASURE.Observation(
            {
                "process_wall_seconds": 0.1,
                "user_cpu_seconds": 1.0,
                "system_cpu_seconds": 0.5,
                "max_rss_bytes": 10_000,
            },
            "",
        )

        def decoded(wall_ns: int) -> object:
            metadata = {
                "input_bytes": 100,
                "width": 512,
                "height": 512,
                "frames": 30,
                "duration_ms": 3_000,
                "iterations": 10,
                "decode_wall_ns": wall_ns,
            }
            return MEASURE.Observation(
                {
                    "process_wall_seconds": 0.2,
                    "user_cpu_seconds": 2.0,
                    "system_cpu_seconds": 1.0,
                    "max_rss_bytes": 14_000,
                },
                json.dumps(metadata),
            )

        with mock.patch.object(
            MEASURE,
            "run_process",
            side_effect=[baseline, decoded(1_000), baseline, decoded(2_000)],
        ):
            result = MEASURE.measure_fixture(
                Path("binary"), Path("fixture"), 10, 2
            )
        self.assertEqual(result["input_bytes"], 100)
        self.assertEqual(len(result["samples"]), 2)
        self.assertEqual(result["median"]["max_rss_delta_bytes"], 4_000)
        self.assertEqual(result["median"]["cpu_ms_per_iteration"], 150)

    def test_summary_requires_samples(self) -> None:
        with self.assertRaises(ValueError):
            MEASURE.summarize([], 10)


if __name__ == "__main__":
    unittest.main()
