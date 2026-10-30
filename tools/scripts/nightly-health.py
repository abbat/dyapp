"""Turns a scheduled workflow run into one tracked GitHub issue.

Run by nightly-health.yml after each scheduled run of the checked workflows:
a failure opens the issue `nightly: <workflow> failing` (label
nightly-failure) or comments on the open one; a green run closes it.
Needs `gh` with GH_TOKEN and GITHUB_REPOSITORY.

    nightly-health.py <run id>
    nightly-health.py --self-check
"""
import json
import os
import subprocess
import sys

LABEL = "nightly-failure"
FAILED = {"failure", "timed_out", "startup_failure"}
# Local reproduction and where to look, per workflow name.
CHECKLISTS = {
    "codeql": ("cargo clippy --workspace --all-features --all-targets "
               "-- -D warnings", "the code scanning alerts and the run log"),
    "fuzz": ("make fuzz", "the fuzz-<target> artifact (corpus and crashes)"),
    "mutants": ("make mutants", "the mutants-<shard> artifacts and "
                "tests/scripts/check-mutants.py"),
    "release-sizes": ("python3 tools/scripts/check-artifact-size.py (see "
                      "release-sizes.yml)", "the release-size-* artifacts and "
                      "docs/development/ci-base.md#artifact-size-budgets"),
    "reproducible": ("python3 -B tests/scripts/check-reproducible.py "
                     "<target>", "the reproducible-<target> artifacts and "
                     "docs/development/ci-base.md#reproducible-builds"),
    "scorecard": ("python3 -B tests/scripts/check-scorecard.py "
                  "scorecard.json", "the scorecard artifact and "
                  "docs/security/assurance.md#openssf-scorecard"),
    "security": ("cargo audit", "the audit-check-output artifact"),
}


def title(workflow):
    return f"nightly: {workflow} failing"


def streak(runs, run_id):
    """Failed scheduled runs in a row, newest first, from run_id back."""
    count, seen = 0, False
    for run in runs:
        seen = seen or run["id"] == run_id
        if not seen or run["conclusion"] in ("cancelled", "skipped"):
            continue
        if run["conclusion"] not in FAILED:
            break
        count += 1
    return count


def failures(jobs):
    """Failed jobs, each with its failed steps."""
    lines = []
    for job in jobs:
        if job["conclusion"] in FAILED:
            steps = [step["name"] for step in job.get("steps") or []
                     if step["conclusion"] in FAILED]
            lines.append(f"- {job['name']}" + (f": {', '.join(steps)}"
                                               if steps else ""))
    return lines


def report(run, jobs, count):
    # Job and step names are data from the run; they are quoted, not run.
    names = "\n".join(failures(jobs)) or "- (no failed job reported)"
    return (f"Scheduled run [{run['id']}]({run['html_url']}) on "
            f"{run['head_sha'][:8]} ended `{run['conclusion']}`; "
            f"{count} failed scheduled run(s) in a row.\n\n"
            f"Failed jobs and steps:\n{names}\n")


def body(workflow, text):
    command, where = CHECKLISTS.get(workflow, ("see the workflow file",
                                               "the run log"))
    return (f"The scheduled `{workflow}` workflow failed.\n\n"
            f"- [ ] Reproduce locally: `{command}`\n"
            f"- [ ] Look at {where}\n"
            "- [ ] Fix it, or record why the failure is accepted\n"
            "- [ ] Close criterion: the next green scheduled run closes "
            "this issue by itself\n\n" + text)


def plan(conclusion, issue):
    """What to do with the issue: open, comment, close or nothing."""
    if conclusion in FAILED:
        return "comment" if issue else "open"
    if conclusion == "success" and issue:
        return "close"
    return None


def gh(*args):
    return subprocess.check_output(["gh", *args], text=True)


def api(path):
    repo = os.environ["GITHUB_REPOSITORY"]
    return json.loads(gh("api", f"repos/{repo}/{path}"))


def main(run_id):
    run = api(f"actions/runs/{run_id}")
    if run["event"] != "schedule":
        print(f"run {run_id} is a {run['event']} run, not scheduled")
        return
    name = title(run["name"])
    issues = json.loads(gh("issue", "list", "--label", LABEL, "--state",
                           "open", "--limit", "100", "--json",
                           "number,title"))
    issue = next((i["number"] for i in issues if i["title"] == name), None)
    action = plan(run["conclusion"], issue)
    print(f"{run['name']} {run['conclusion']}: {action or 'nothing'}")
    if action == "close":
        gh("issue", "close", str(issue), "--comment",
           f"Scheduled run [{run['id']}]({run['html_url']}) is green.")
    if action not in ("open", "comment"):
        return
    runs = api(f"actions/workflows/{run['workflow_id']}/runs"
               "?event=schedule&status=completed&per_page=50")
    jobs = api(f"actions/runs/{run_id}/jobs?per_page=100")["jobs"]
    text = report(run, jobs, streak(runs["workflow_runs"], run["id"]))
    if action == "comment":
        gh("issue", "comment", str(issue), "--body", text)
        return
    gh("label", "create", LABEL, "--force", "--color", "B60205",
       "--description", "A scheduled workflow is failing")
    gh("issue", "create", "--title", name, "--label", LABEL, "--body",
       body(run["name"], text))


def self_check():
    runs = [{"id": 5, "conclusion": "failure"},
            {"id": 4, "conclusion": "timed_out"},
            {"id": 3, "conclusion": "cancelled"},
            {"id": 2, "conclusion": "failure"},
            {"id": 1, "conclusion": "success"},
            {"id": 0, "conclusion": "failure"}]
    assert streak(runs, 5) == 3
    assert streak(runs, 4) == 2
    assert streak(runs, 1) == 0
    jobs = [{"name": "a", "conclusion": "success", "steps": []},
            {"name": "b", "conclusion": "failure",
             "steps": [{"name": "x", "conclusion": "success"},
                       {"name": "y", "conclusion": "failure"}]},
            {"name": "c", "conclusion": "timed_out", "steps": None}]
    assert failures(jobs) == ["- b: y", "- c"]
    assert plan("failure", None) == "open"
    assert plan("timed_out", 7) == "comment"
    assert plan("success", 7) == "close"
    assert plan("success", None) is None
    assert plan("cancelled", 7) is None
    assert "make fuzz" in body("fuzz", "")
    assert set(CHECKLISTS) == {"codeql", "fuzz", "mutants", "release-sizes",
                               "reproducible", "scorecard", "security"}
    print("OK nightly-health self-check")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-check"]:
        self_check()
    else:
        main(sys.argv[1])
