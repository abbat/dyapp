#!/usr/bin/env python3
"""Require complete LLVM coverage scope and >=70% line coverage."""
import json
from pathlib import Path
import sys

MIN_COVERAGE = 70
CRATES = (
    "bootstrap", "ffi", "identity", "messaging", "p2p-net", "profile", "video",
)


def validate(report):
    if report.get("type") != "llvm.coverage.json.export":
        raise ValueError("Not an LLVM coverage JSON export")
    data = report.get("data")
    if not isinstance(data, list) or len(data) != 1:
        raise ValueError("Missing or ambiguous coverage dataset")
    files = data[0].get("files")
    if not isinstance(files, list) or not files:
        raise ValueError("Empty coverage file inventory")
    paths = [item["filename"].replace("\\", "/") for item in files]
    missing = [crate for crate in CRATES
               if not any(f"/rust/{crate}/src/" in path for path in paths)]
    if missing:
        raise ValueError(f"Partial workspace coverage: missing {missing}")
    lines = data[0]["totals"]["lines"]
    count, covered = lines["count"], lines["covered"]
    if type(count) is not int or type(covered) is not int:
        raise ValueError("Invalid covered line counts")
    if count <= 0 or not 0 <= covered <= count:
        raise ValueError("Empty or invalid covered line counts")
    percentage = covered * 100 / count
    if covered * 100 < MIN_COVERAGE * count:
        raise ValueError(f"Line coverage {percentage:.2f}% < {MIN_COVERAGE}%")
    return percentage


if __name__ == "__main__":
    percentage = validate(json.loads(Path(sys.argv[1]).read_text()))
    print(f"Complete Rust workspace: {percentage:.2f}% line coverage")
