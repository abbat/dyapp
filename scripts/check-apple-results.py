"""Reject missing, empty, failed or skipped native XCTest inventories."""
import json
from pathlib import Path
import sys


def validate(summary):
    keys = ("totalTestCount", "passedTests", "failedTests", "skippedTests")
    if any(type(summary.get(key)) is not int for key in keys):
        raise ValueError("Missing or invalid xcresult summary counts")
    if summary["totalTestCount"] < 3:
        raise ValueError("Incomplete native unit/UI inventory")
    if summary["failedTests"] or summary["skippedTests"]:
        raise ValueError("Failed or skipped native tests")
    if summary["passedTests"] != summary["totalTestCount"]:
        raise ValueError("Unfinished native tests")
    if summary.get("result") != "Passed":
        raise ValueError("Native test result is not Passed")
    return summary["passedTests"]


if __name__ == "__main__":
    count = validate(json.loads(Path(sys.argv[1]).read_text()))
    print(f"{count} native tests passed")
