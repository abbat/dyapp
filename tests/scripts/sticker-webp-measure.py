"""Measure real sticker WebP first-frame decoding without external tools."""

import argparse
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time
from typing import NamedTuple

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ("static-512.webp", "animated-512-30f-3000ms.webp")
CAVEATS = [
    "Peak RSS is process-wide and retains allocator high-water marks; "
    "it is not a decoder allocation bound.",
    "CPU and wall deltas include residual process noise after subtracting "
    "a same-binary read-only baseline.",
    "Only the first frame is decoded; animation playback and later-frame "
    "decode costs are out of scope.",
]


class Observation(NamedTuple):
    values: dict[str, float | int]
    output: str


def run_process(binary: Path, args: list[str]) -> Observation:
    started = time.monotonic()
    with tempfile.TemporaryFile() as output:
        process = subprocess.Popen(
            [str(binary), *args],
            stdout=output,
            stderr=subprocess.STDOUT,
        )
        _, status, usage = os.wait4(process.pid, 0)
        process.returncode = os.waitstatus_to_exitcode(status)
        output.seek(0)
        text = output.read().decode()
    if process.returncode:
        raise RuntimeError(
            f"measurement child failed ({process.returncode}):\n{text}"
        )
    values: dict[str, float | int] = {
        "process_wall_seconds": time.monotonic() - started,
        "user_cpu_seconds": usage.ru_utime,
        "system_cpu_seconds": usage.ru_stime,
        "max_rss_bytes": usage.ru_maxrss * 1024,
    }
    return Observation(values, text)


def decode_metadata(text: str, iterations: int) -> dict[str, int]:
    try:
        raw = json.loads(text)
    except json.JSONDecodeError as error:
        raise ValueError(
            f"decoder returned malformed JSON: {text!r}"
        ) from error
    if not isinstance(raw, dict):
        raise ValueError("decoder metadata must be a JSON object")
    result: dict[str, int] = {}
    for key in (
        "input_bytes",
        "width",
        "height",
        "frames",
        "duration_ms",
        "iterations",
        "decode_wall_ns",
    ):
        value = raw.get(key)
        if type(value) is not int:
            raise ValueError(f"decoder metadata {key} must be an integer")
        result[key] = value
    if result["iterations"] != iterations:
        raise ValueError(
            "decoder metadata iteration count differs from the request"
        )
    if (
        result["input_bytes"] <= 0
        or result["width"] <= 0
        or result["height"] <= 0
    ):
        raise ValueError("decoder metadata contains empty input or dimensions")
    return result


def summarize(
    samples: list[dict[str, float | int]], iterations: int
) -> dict[str, float | int]:
    if not samples:
        raise ValueError("at least one measurement sample is required")
    cpu = [
        (sample["user_cpu_seconds"] + sample["system_cpu_seconds"])
        * 1000
        / iterations
        for sample in samples
    ]
    return {
        "cpu_ms_per_iteration": statistics.median(cpu),
        "decode_wall_ms_per_iteration": statistics.median(
            sample["decode_wall_ns"] for sample in samples
        )
        / iterations
        / 1_000_000,
        "process_wall_seconds": statistics.median(
            sample["process_wall_seconds"] for sample in samples
        ),
        "max_rss_bytes": int(
            statistics.median(sample["max_rss_bytes"] for sample in samples)
        ),
        "max_rss_delta_bytes": int(
            statistics.median(
                sample["max_rss_delta_bytes"] for sample in samples
            )
        ),
    }


def measure_fixture(
    binary: Path,
    fixture: Path,
    iterations: int,
    sample_count: int,
) -> dict[str, object]:
    samples: list[dict[str, float | int]] = []
    expected: dict[str, int] | None = None
    for _ in range(sample_count):
        baseline = run_process(binary, ["baseline", str(fixture)])
        decoded = run_process(
            binary, ["decode", str(fixture), str(iterations)]
        )
        metadata = decode_metadata(decoded.output, iterations)
        stable = {
            key: value
            for key, value in metadata.items()
            if key != "decode_wall_ns"
        }
        if expected is None:
            expected = stable
        elif stable != expected:
            raise ValueError(
                f"fixture metadata changed between samples: {fixture}"
            )
        sample = {
            key: decoded.values[key] - baseline.values[key]
            for key in (
                "user_cpu_seconds",
                "system_cpu_seconds",
                "max_rss_bytes",
            )
        }
        sample["max_rss_delta_bytes"] = sample.pop("max_rss_bytes")
        sample["process_wall_seconds"] = decoded.values["process_wall_seconds"]
        sample["max_rss_bytes"] = decoded.values["max_rss_bytes"]
        sample["decode_wall_ns"] = metadata["decode_wall_ns"]
        samples.append(sample)
    assert expected is not None
    return {
        **expected,
        "samples": samples,
        "median": summarize(samples, iterations),
    }


def source_environment() -> dict[str, str]:
    return {
        "revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        "rustc": subprocess.check_output(["rustc", "-Vv"], text=True).strip(),
        "platform": platform.platform(),
        "machine": platform.machine(),
    }


def measure(
    binary: Path, work_dir: Path, iterations: int, samples: int
) -> dict[str, object]:
    if platform.system() != "Linux":
        raise ValueError(
            "sticker WebP resource accounting currently requires "
            "Linux wait4/rusage"
        )
    run_process(binary, ["generate", str(work_dir)])
    fixtures = [
        measure_fixture(binary, work_dir / name, iterations, samples)
        for name in FIXTURES
    ]
    return {
        "schema": 1,
        **source_environment(),
        "iterations_per_sample": iterations,
        "samples_per_fixture": samples,
        "fixtures": fixtures,
        "caveats": CAVEATS,
        "errors": [],
    }


def positive(value: str) -> int:
    number = int(value)
    if not 1 <= number <= 10_000:
        raise argparse.ArgumentTypeError("value must be in 1..=10000")
    return number


def write_report(path: Path, report: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--work-dir", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--iterations", type=positive, default=100)
    parser.add_argument("--samples", type=positive, default=3)
    args = parser.parse_args()
    try:
        report = measure(
            args.binary, args.work_dir, args.iterations, args.samples
        )
    except (
        OSError,
        RuntimeError,
        ValueError,
        subprocess.CalledProcessError,
    ) as error:
        report = {"schema": 1, "errors": [str(error)]}
        write_report(args.report, report)
        print(f"FAIL sticker-webp-measurement: {error}")
        return 1
    write_report(args.report, report)
    print(f"OK sticker-webp-measurement: {args.report}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
