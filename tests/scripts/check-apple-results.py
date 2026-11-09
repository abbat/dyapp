"""Reject missing, empty, failed or skipped native XCTest inventories."""

import json
from pathlib import Path
import sys
from collections.abc import Mapping
from typing import TypedDict, cast


class XCTestCounts(TypedDict):
    totalTestCount: int
    passedTests: int
    failedTests: int
    skippedTests: int


def validate(summary: Mapping[str, object]) -> int:
    keys = ("totalTestCount", "passedTests", "failedTests", "skippedTests")
    if any(type(summary.get(key)) is not int for key in keys):
        raise ValueError("Missing or invalid xcresult summary counts")
    counts = cast(XCTestCounts, summary)
    if counts["totalTestCount"] < 3:
        raise ValueError("Incomplete native unit/UI inventory")
    if counts["failedTests"] or counts["skippedTests"]:
        raise ValueError("Failed or skipped native tests")
    if counts["passedTests"] != counts["totalTestCount"]:
        raise ValueError("Unfinished native tests")
    if summary.get("result") != "Passed":
        raise ValueError("Native test result is not Passed")
    return counts["passedTests"]


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("apple-results")
    count = validate(json.loads(Path(sys.argv[1]).read_text()))
    print(f"{count} native tests passed")
