"""Persist complete observations for the existing RSS plateau policy."""
import hashlib
import json
from pathlib import Path
import uuid


def write_report(samples, elapsed, seconds, growth, binary, directory):
    if elapsed < seconds:
        raise RuntimeError("Incomplete plateau observation")
    half = len(samples) // 2
    if half < 1:
        raise RuntimeError("Too few rounds for a plateau")
    nodes = []
    for index in range(len(samples[0]["rss_kib"])):
        first = max(sample["rss_kib"][index] for sample in samples[:half])
        second = max(sample["rss_kib"][index] for sample in samples[half:])
        nodes.append({"node": index + 1, "first_peak_kib": first,
                      "second_peak_kib": second,
                      "failed": second > first * (1 + growth)})
    with Path(binary).open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    report = {"duration_limit_seconds": seconds,
              "measured_seconds": elapsed, "growth_limit": growth,
              "binary_sha256": digest, "rounds": len(samples),
              "nodes": nodes, "samples": samples,
              "failed_nodes": [node["node"] for node in nodes
                               if node["failed"]]}
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / f"memory-plateau-{uuid.uuid4().hex}.json"
    with path.open("x") as output:
        output.write(json.dumps(report, indent=2) + "\n")
    return path, report
