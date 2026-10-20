"""Local A7 report; reads bd/git/gh, never writes to trackers or GitHub."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import re
import subprocess

from flow_metrics import health, instant, report

ROOT = Path(__file__).resolve().parents[1]


def command(*args):
    for _ in range(3 if args[0] == "gh" else 1):
        result = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
        if result.returncode == 0:
            return result.stdout
    raise RuntimeError(f"{args[0]} failed: {result.stderr.strip()}")


def git(*args):
    return command("git", "-c", f"safe.directory={ROOT}", *args)


def pages(endpoint, key):
    text = command("gh", "api", "--paginate", "--slurp", endpoint)
    result = json.loads(text)
    records = [row for page in result for row in page[key]]
    ids = {row["id"] for row in records}
    if not result or len(ids) != result[0]["total_count"]:
        raise ValueError(f"Incomplete GitHub pagination: {key}")
    if len(ids) != len(records):
        raise ValueError(f"Duplicate GitHub records: {key}")
    return records


def alerts(repo):
    """A8 triggers: Dependabot alerts and repository security advisories."""
    found = []
    for endpoint, ghsa in (
            (f"repos/{repo}/dependabot/alerts?per_page=100&state="
             "auto_dismissed,dismissed,fixed,open",
             lambda row: row["security_advisory"]["ghsa_id"]),
            (f"repos/{repo}/security-advisories?per_page=100",
             lambda row: row["ghsa_id"])):
        pages = json.loads(command("gh", "api", "--paginate", "--slurp",
                                   endpoint))
        found += [{"ghsa_id": ghsa(row), "created_at": row["created_at"]}
                  for page in pages for row in page]
    return found


def adoption_dates():
    history = git("log", "--reverse", "--format=%x00%cI", "-p", "main", "--",
                  "docs/development/practices.md")
    found, date = {}, None
    for line in history.splitlines():
        if line.startswith("\0"):
            date = line[1:]
        match = re.match(r"^\+\| ([A-Z]\d+[a-z]?) \(adopted\)", line)
        if match:
            found.setdefault(match[1], date)
    return [{"id": key, "adopted_at": date} for key, date in found.items()]


def commits(since):
    text = git("log", "main", "--no-merges", f"--since={since}",
               "--format=%H%x00%cI%x00%B%x00%(trailers)%x00")
    result = []
    fields = text.strip().split("\0") if text.strip() else []
    for offset in range(0, len(fields) - 1, 4):
        sha, at, message, trailers = fields[offset:offset + 4]
        sha = sha.strip()
        diff = git("show", "--format=", "--unified=0", "--no-ext-diff",
                   "--no-textconv", "--no-renames", sha)
        added = "\n".join(line[1:] for line in diff.splitlines()
                          if all((line.startswith("+"),
                                  not line.startswith("+++"))))
        annotations = {}
        patterns = {"mutants::skip": r"^\s*#\[mutants::skip\b",
                    "gitleaks:allow": r"^\s*(?://|#).*gitleaks:allow\b"}
        for key, pattern in patterns.items():
            count = len(re.findall(pattern, added, re.M))
            if count:
                annotations[key] = count
        result.append({"sha": sha, "at": at, "message": message,
                       "trailers": trailers, "annotations": annotations})
    return result


def collect(now, ledger, full=True):
    if git("rev-parse", "--is-shallow-repository").strip() != "false":
        raise ValueError("Full Git history is required")
    since = (instant(now) - timedelta(days=7)).isoformat()
    repo = command("gh", "repo", "view", "--json", "nameWithOwner",
                   "--jq", ".nameWithOwner").strip()
    endpoint = f"repos/{repo}/actions/runs?per_page=100"
    all_runs = pages(endpoint, "workflow_runs")
    runs = [r for r in all_runs if all((
        r["head_branch"] == "main", r["event"] == "push",
        instant(r["created_at"]) <= instant(now)))]
    prior = [r for r in runs if all((
        r["path"] == ".github/workflows/ci.yml", r["conclusion"] == "success",
        instant(r["updated_at"]) < instant(since)))]
    anchor = max((r["created_at"] for r in prior), default=None)
    root_time = git("log", "main", "--reverse", "--format=%cI").splitlines()[0]
    if anchor is None and instant(root_time) < instant(since):
        raise ValueError("Missing pre-week green CI anchor")
    selected = [r for r in runs if any((
        instant(r["updated_at"]) >= instant(since), r["status"] != "completed",
        all((r["path"] == ".github/workflows/ci.yml",
             anchor is not None, r["created_at"] >= (anchor or "")))))]
    api = f"repos/{repo}/actions/runs"

    def details(run):
        attempts = []
        for number in range(1, run["run_attempt"] + 1):
            endpoint = f"{api}/{run['id']}/attempts/{number}"
            meta = run if number == run["run_attempt"] else json.loads(
                command("gh", "api", endpoint))
            jobs = pages(endpoint + "/jobs?per_page=100", "jobs")
            finished = None
            if meta["status"] == "completed":
                finished = max((j["completed_at"] for j in jobs
                                if j["completed_at"]), default=None)
                finished = finished or meta["updated_at"]
            attempts.append({
                "id": run["id"], "attempt": number, "jobs": jobs,
                "created_at": run["created_at"], "completed_at": finished,
                "head_sha": run["head_sha"], "conclusion": meta["conclusion"],
                "workflow": run["path"]})
        return attempts

    with ThreadPoolExecutor(max_workers=4) as pool:
        attempts = [a for group in pool.map(details, selected) for a in group]
    snapshot = {
        "schema": 1, "now": now, "runs": attempts,
        "sources": {"repository": repo,
                    "main": git("rev-parse", "main").strip(),
                    "github_runs": len(all_runs),
                    "selected_runs": len(selected)},
    }
    if full:
        snapshot.update({
            "beads": json.loads(command("bd", "list", "--all", "--limit", "0",
                                        "--json", "--brief")),
            "commits": commits(since), "practices": adoption_dates(),
            "alerts": alerts(repo),
            "observations": ledger.get("observations", []),
            "reviews": ledger.get("reviews", [])})
    return snapshot


def write(path, value):
    text = json.dumps(value, indent=2) + "\n"
    if path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", type=Path, help="offline input JSON")
    parser.add_argument("--snapshot-out", type=Path, help="save live input")
    parser.add_argument("--ledger", type=Path, help="local review ledger")
    parser.add_argument("--report", type=Path, help="output JSON")
    parser.add_argument("--health", action="store_true",
                        help="daily main CI check only: exit 2 on a warning")
    args = parser.parse_args()
    try:
        if args.snapshot:
            snapshot = json.loads(args.snapshot.read_text())
        else:
            ledger = json.loads(args.ledger.read_text()) if args.ledger else {}
            snapshot = collect(datetime.now(timezone.utc).isoformat(), ledger,
                               full=not args.health)
            write(args.snapshot_out, snapshot)
        result = (health if args.health else report)(snapshot)
    except (ValueError, KeyError, TypeError, OSError, RuntimeError) as error:
        result = {"errors": [str(error)]}
    print(write(args.report, result), end="")
    if result["errors"]:
        return 1
    return 2 if args.health and result["warnings"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
