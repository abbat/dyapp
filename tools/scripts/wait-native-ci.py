#!/usr/bin/env python3
"""Wait for the Apple UI tests of a release tag's own CI run.

Usage: wait-native-ci.py TAG COMMIT
Needs gh authentication. The tag's ci.yml run already holds these tests, so a
release does not take scarce macOS runners to repeat them. The job's time limit
bounds the wait, as a run can queue for hours.
"""

import json
import subprocess
import sys
import time


REPO = "abbat/dyapp"
REQUIRED = {"native - macos", "native - ios"}
POLL_SECONDS = 60


def verdict(runs):
    """One value per run: its job list. A rerun shows as jobs not yet completed."""
    waiting = not runs
    for jobs in runs:
        found = {job["name"]: job for job in jobs if job["name"] in REQUIRED}
        if set(found) != REQUIRED:
            waiting = True
        elif all(job["conclusion"] == "success" for job in found.values()):
            return "passed"
        elif any(job["status"] != "completed" for job in found.values()):
            waiting = True
    return "waiting" if waiting else "failed"


def api(path):
    return json.loads(subprocess.run(
        ["gh", "api", path], check=True, capture_output=True, timeout=60
    ).stdout)


def tag_runs(tag, commit, fetch=api):
    listed = fetch(f"repos/{REPO}/actions/workflows/ci.yml/runs"
                   f"?event=push&head_sha={commit}&branch={tag}")
    return [fetch(f"repos/{REPO}/actions/runs/{run['id']}/jobs?per_page=100")
            ["jobs"] for run in listed["workflow_runs"]]


def main():
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    while True:
        try:
            result = verdict(tag_runs(sys.argv[1], sys.argv[2]))
        except (OSError, ValueError, KeyError, TypeError,
                subprocess.SubprocessError) as error:
            # A passing gate must not be lost to one failed request.
            print(f"retrying after: {error}", flush=True)
            result = "waiting"
        print(result, flush=True)
        if result != "waiting":
            return 0 if result == "passed" else 1
        time.sleep(POLL_SECONDS)


if __name__ == "__main__":
    sys.exit(main())
