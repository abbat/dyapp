"""Practice B5: fail on flaky tests outside a fresh quarantine.

A test is flaky when it failed and then passed on retry: `flakyFailure` or
`flakyError` in a cargo-nextest JUnit report (.xml), a failed Repetition under
a passed Test Case in `xcresulttool get test-results tests` (.json), or a
failure listed by an Android `am instrument` run (.txt) that a rerun of the
suite passed. Each one needs an
entry in [flaky] of tests/scripts/baselines.toml, dated the day it was
found; an
entry stops covering its test after QUARANTINE_DAYS.
"""
from datetime import date, timedelta
from pathlib import Path
import json
import re
import sys
import tomllib
import xml.etree.ElementTree as ET

BASELINES = Path(__file__).with_name("baselines.toml")
QUARANTINE_DAYS = 7


def flaky_tests(report):
    """Tests in a JUnit report that passed only on a retry."""
    return sorted(
        f"{case.get('classname')} {case.get('name')}"
        for case in ET.fromstring(report).iter("testcase")
        if any(case.find(tag) is not None
               for tag in ("flakyFailure", "flakyError")))


def xcresult_flaky(report, suite=""):
    """Test Cases in an xcresult tests tree that passed after a failure."""
    found = []
    for node in report.get("testNodes", report.get("children", [])):
        children = node.get("children", [])
        if node.get("nodeType") == "Test Case":
            runs = [child.get("result") for child in children
                    if child.get("nodeType") == "Repetition"]
            if node.get("result") == "Passed" and "Failed" in runs:
                found.append(f"{suite} {node.get('name')}")
        else:
            found += xcresult_flaky(
                {"children": children}, node.get("name", suite)
                if node.get("nodeType") == "Test Suite" else suite)
    return sorted(found)


def instrumentation_failures(output):
    """Tests listed as failed by `am instrument -w`; the run if none is."""
    return sorted(f"{cls} {name}" for name, cls in re.findall(
        r"^\d+\) (\w+)\(([\w.$]+)\)$", output, re.MULTILINE)
    ) or ["android instrumentation"]


def report_flaky(path):
    """Flaky tests in a report, by its type."""
    if path.suffix == ".json":
        return xcresult_flaky(json.loads(path.read_text()))
    if path.suffix == ".txt":
        return instrumentation_failures(path.read_text())
    return flaky_tests(path.read_bytes())


def flaky_errors(flaky, quarantine, today):
    """Unlisted flaky tests and quarantine entries past their deadline."""
    errors = [f"{test}: flaky, open a bead and add it to [flaky] in "
              f"baselines.toml with {today.isoformat()}"
              for test in flaky if test not in quarantine]
    oldest = today - timedelta(days=QUARANTINE_DAYS)
    errors += [f"{test}: quarantined since {found}, more than "
               f"{QUARANTINE_DAYS} days; fix it and remove the entry"
               for test, found in sorted(quarantine.items()) if found < oldest]
    return errors


if __name__ == "__main__":
    from check_wrapper import wrap
    wrap("flaky")
    flaky = []
    for name in sys.argv[1:]:
        flaky += report_flaky(Path(name))
    quarantine = tomllib.loads(BASELINES.read_text()).get("flaky", {})
    errors = flaky_errors(flaky, quarantine, date.today())
    if errors:
        raise RuntimeError("Flaky tests:\n" + "\n".join(errors))
    print(f"{len(flaky)} flaky tests, all in a fresh quarantine")
