"""Practice B5: fail on flaky tests outside a fresh quarantine.

A test is flaky when a JUnit report shows it failed and then passed on retry
(`flakyFailure`/`flakyError`, as written by cargo-nextest). Each one needs an
entry in [flaky] of scripts/baselines.toml, dated the day it was found; an
entry stops covering its test after QUARANTINE_DAYS.
"""
from datetime import date, timedelta
from pathlib import Path
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
    from check_dispatch import wrap
    wrap("flaky")
    flaky = []
    for name in sys.argv[1:]:
        flaky += flaky_tests(Path(name).read_bytes())
    quarantine = tomllib.loads(BASELINES.read_text()).get("flaky", {})
    errors = flaky_errors(flaky, quarantine, date.today())
    if errors:
        raise RuntimeError("Flaky tests:\n" + "\n".join(errors))
    print(f"{len(flaky)} flaky tests, all in a fresh quarantine")
