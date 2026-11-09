"""Persist complete observations for the existing RSS plateau policy."""

from __future__ import annotations

from collections.abc import Sequence
from typing import TYPE_CHECKING

import os

import hashlib
import json
from pathlib import Path
import uuid


if TYPE_CHECKING:
    import network_contracts as nc


def write_report(
    samples: Sequence[nc.MemorySample],
    elapsed: "float",
    seconds: "int",
    growth: "float",
    binary: "Path | str",
    directory: "str | os.PathLike[str]",
) -> tuple[Path, nc.MemoryReport]:
    if elapsed < seconds:
        raise RuntimeError("Incomplete plateau observation")
    half = len(samples) // 2
    if half < 1:
        raise RuntimeError("Too few rounds for a plateau")
    nodes: list[nc.MemoryNode] = []
    for index in range(len(samples[0]["rss_kib"])):
        first = max(sample["rss_kib"][index] for sample in samples[:half])
        second = max(sample["rss_kib"][index] for sample in samples[half:])
        nodes.append(
            {
                "node": index + 1,
                "first_peak_kib": first,
                "second_peak_kib": second,
                "failed": second > first * (1 + growth),
            }
        )
    with Path(binary).open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    report: nc.MemoryReport = {
        "duration_limit_seconds": seconds,
        "measured_seconds": elapsed,
        "growth_limit": growth,
        "binary_sha256": digest,
        "rounds": len(samples),
        "nodes": nodes,
        "samples": samples,
        "failed_nodes": [node["node"] for node in nodes if node["failed"]],
    }
    destination = Path(directory)
    destination.mkdir(parents=True, exist_ok=True)
    path = destination / f"memory-plateau-{uuid.uuid4().hex}.json"
    with path.open("x") as output:
        output.write(json.dumps(report, indent=2) + "\n")
    return path, report
