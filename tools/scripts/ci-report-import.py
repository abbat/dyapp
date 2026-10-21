"""Practice A12: turn failing CI jobs into beads, without duplicates.

Usage: ci-report-import.py [--dry-run] [RUN-ID ...]
Runs on the host (needs an authenticated `gh` and `bd`); CI never touches
beads. Without run ids it reads the latest completed run of every workflow on
main, so failures already fixed by a later run are not imported.

One bead per failing job, keyed by the external ref `ci:<workflow>/<job>`.
A repeated failure appends a note (once per run) and reopens a closed bead.
"""
import json
import re
import subprocess
import sys

BRANCH = "main"
ERROR = re.compile(r"error|fail|panicked|denied|warning: unused", re.I)
# Colour codes arrive as an escape byte or as literal "^[".
ANSI = re.compile(r"(?:\x1b|\^\[)\[[0-9;]*m")
# Passing tests, and the step header: echoed script (cyan) and env block.
NOISE = re.compile(r"\.\.\. ok$|\b0 failed\b|^  [\w-]+: "
                   r"|^(?:\x1b|\^\[)\[36;1m")
# `job<TAB>step<TAB><timestamp> text` lines of `gh run view --log-failed`.
LOG = re.compile(r"^([^\t]*)\t([^\t]*)\t\ufeff?\S+Z ?(.*)$")
EXCERPT = 30
# ci.yml `summary`: the branch-protection check that needs every other job.
AGGREGATE = {"required"}


def run(*args):
    return subprocess.check_output(args, text=True)


def latest(runs):
    """Ids of the newest completed run per workflow (runs newest first)."""
    seen = {}
    for item in runs:
        if item["status"] == "completed":
            seen.setdefault(item["workflowName"], item["databaseId"])
    return list(seen.values())


def failures(view):
    """(ref, title, job, step) for each failed job of a `gh run view`."""
    failed = [job for job in view["jobs"]
              if job["conclusion"] in ("failure", "timed_out")]
    result = []
    for job in failed:
        # An aggregate job fails because others did: report those instead.
        if job["name"] in AGGREGATE and len(failed) > 1:
            continue
        steps = [s["name"] for s in job["steps"]
                 if s["conclusion"] == "failure"] or ["(no step)"]
        ref = f"ci:{view['workflowName']}/{job['name']}"
        title = f"CI {view['workflowName']} / {job['name']} fails: {steps[0]}"
        result.append((ref, title, job["name"], steps[0]))
    return result


def excerpt(log, job):
    """Error lines of one job's failed log, or its tail when none match."""
    # The step column of `--log-failed` reads "UNKNOWN STEP", so filter by job.
    lines = [m.group(3) for m in map(LOG.match, log.splitlines())
             if m and m.group(1) == job]
    errors = [line for line in lines
              if ERROR.search(line) and not NOISE.search(line)]
    return "\n".join(ANSI.sub("", line)
                     for line in (errors or lines)[-EXCERPT:])


def report(view, step, text):
    return (f"Run {view['databaseId']} ({view['event']}, "
            f"{view['headSha'][:12]}): {view['url']}\nFailed step: {step}\n"
            f"Details: gh run view {view['databaseId']} --log-failed\n\n"
            f"{text}")


def action(existing, run_id):
    """What to do with a failure given the beads that share its ref."""
    if not existing:
        return "create"
    bead = existing[0]
    if f"Run {run_id} " in (bead.get("notes") or "") + bead["description"]:
        return "skip"
    return "reopen" if bead["status"] == "closed" else "note"


def main(args):
    dry = "--dry-run" in args
    ids = [a for a in args if a.isdigit()]
    if not ids:
        ids = latest(json.loads(run(
            "gh", "run", "list", "--branch", BRANCH, "--limit", "100",
            "--json", "databaseId,workflowName,status")))
    for run_id in ids:
        view = json.loads(run(
            "gh", "run", "view", str(run_id), "--json",
            "databaseId,workflowName,jobs,headSha,url,event"))
        found = failures(view)
        log = run("gh", "run", "view", str(run_id), "--log-failed") \
            if found else ""
        for ref, title, job, step in found:
            existing = json.loads(run("bd", "list", "--external-ref", ref,
                                      "--all", "--json"))
            todo = action(existing, view["databaseId"])
            text = report(view, step, excerpt(log, job))
            print(f"{todo}: {ref} (run {run_id})")
            if dry or todo == "skip":
                continue
            if todo == "create":
                run("bd", "create", title, "-t", "bug", "-p", "1",
                    "-l", "bug,infra", "--external-ref", ref, "--silent",
                    "--description", text)
                continue
            if todo == "reopen":
                run("bd", "reopen", existing[0]["id"])
            run("bd", "note", existing[0]["id"], text)
    print("OK")


if __name__ == "__main__":
    main(sys.argv[1:])
