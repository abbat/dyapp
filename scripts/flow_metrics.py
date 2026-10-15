"""Pure A7 calculations over a local, versioned snapshot schema."""
from collections import Counter
from datetime import datetime, timedelta, timezone
import math

HATCHES = {"Large-Change", "Size-Increase", "Docs-Unchanged", "Quorum"}
BAD = {"failure", "timed_out", "startup_failure", "action_required", "stale"}


def instant(value):
    result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if result.tzinfo is None:
        raise ValueError("Timestamps must include a timezone")
    return result.astimezone(timezone.utc)


def percentile(values, fraction):
    return sorted(values)[math.ceil(len(values) * fraction) - 1] \
        if values else None


def ratio(count, total):
    return 100 * count / total if total else None


def ci_metrics(runs, since, now):
    ci = [r for r in runs if r["workflow"] == ".github/workflows/ci.yml"]
    if not ci:
        raise ValueError("No canonical main CI history")
    events = [r for r in ci if r["completed_at"]]
    events.sort(key=lambda r: instant(r["completed_at"]))
    red, generation, green_generation = None, None, None
    intervals, recovery, outcomes = [], [], {}
    for run in events:
        end = instant(run["completed_at"])
        if end > now:
            continue
        order = (instant(run["created_at"]), run.get("id", 0), run["attempt"])
        conclusion = run["conclusion"]
        if since <= end < now and conclusion in BAD | {"success"}:
            outcomes.setdefault(run["head_sha"], set()).add(conclusion)
        if conclusion == "success":
            green_generation = max(green_generation or order, order)
            if red is not None and order >= generation:
                intervals.append((red, end))
                if end >= since:
                    recovery.append((end - red).total_seconds() / 3600)
                red, generation = None, None
        elif conclusion in BAD:
            if green_generation is not None and order <= green_generation:
                continue
            red = red or end
            generation = max(generation or order, order)
    if red is not None:
        intervals.append((red, now))
    red_hours = [(end - start).total_seconds() / 3600
                 for start, end in intervals if end >= since]
    minutes = 0
    for run in runs:
        for job in run["jobs"]:
            if not job.get("started_at"):
                continue
            start = instant(job["started_at"])
            end = instant(job["completed_at"]) if job.get("completed_at") \
                else now
            if end < start:
                raise ValueError("Job completion precedes start")
            clipped_start, clipped_end = max(start, since), min(end, now)
            elapsed = (clipped_end - clipped_start).total_seconds()
            minutes += max(0, elapsed) / 60
    failed = sum(bool(results & BAD) for results in outcomes.values())
    return {
        "red_commit_percent": ratio(failed, len(outcomes)),
        "failed_commits": failed, "completed_commits": len(outcomes),
        "max_red_hours": max(red_hours, default=0),
        "current_red_hours": (now - red).total_seconds() / 3600
        if red is not None else 0,
        "time_to_green_hours_p50": percentile(recovery, .5),
        "time_to_green_hours_p85": percentile(recovery, .85),
        "recovered_intervals": len(recovery),
        "job_minutes": minutes,
        "pending_runs": sum(not r["completed_at"] for r in runs),
    }


def report(snapshot):
    if snapshot["schema"] != 1:
        raise ValueError("Unsupported snapshot schema")
    now = instant(snapshot["now"])
    since = now - timedelta(days=7)

    def within(value):
        return since <= instant(value) < now
    errors, leads, wip = [], [], Counter()
    completed = 0
    for bead in snapshot["beads"]:
        if bead["status"] == "in_progress":
            agent = bead.get("assignee") or "(unassigned)"
            wip[agent] += 1
            if agent == "(unassigned)":
                errors.append("Unassigned work in progress")
        if bead["issue_type"] in {"epic", "molecule", "convoy", "gate"}:
            continue
        if bead["status"] == "closed" and within(bead["closed_at"]):
            created = instant(bead["created_at"])
            closed = instant(bead["closed_at"])
            hours = (closed - created).total_seconds() / 3600
            if hours < 0:
                raise ValueError("Bead closure precedes creation")
            leads.append(hours)
            completed += 1
    errors += [f"WIP {agent}: {count} > 3" for agent, count in wip.items()
               if count > 3]
    ci = ci_metrics(snapshot["runs"], since, now)
    if ci["max_red_hours"] > 24:
        errors.append(f"Main red {ci['max_red_hours']:.2f} hours > 24")
    commits = [c for c in snapshot["commits"] if within(c["at"])]
    hatches, hatch_commits, reverts = Counter(), 0, 0
    for commit in commits:
        used = Counter()
        for line in commit["trailers"].splitlines():
            key, sep, value = line.partition(":")
            if sep and key in HATCHES and value.strip():
                used[key] += 1
        used.update(commit.get("annotations", {}))
        hatches.update(used)
        hatch_commits += bool(used)
        reverts += 'This reverts commit ' in commit["message"]
    checks = {}
    for sample in snapshot["observations"]:
        if not isinstance(sample["false_positive"], bool):
            raise ValueError("false_positive must be a JSON boolean")
        if within(sample["at"]):
            counts = checks.setdefault(sample["check"], [0, 0])
            counts[0] += 1
            counts[1] += sample["false_positive"]
    false_positives = {
        name: {"samples": total, "false_positives": false,
               "percent": ratio(false, total)}
        for name, (total, false) in checks.items()}
    reviews = []
    for practice in snapshot["practices"]:
        adopted = instant(practice["adopted_at"])
        due = adopted + timedelta(days=28)
        done = [r for r in snapshot["reviews"]
                if all((r["practice"] == practice["id"],
                        due <= instant(r["at"]) <= now))]
        reviews.append({"practice": practice["id"],
                        "due_at": due.isoformat(),
                        "reviewed": bool(done),
                        "overdue": due <= now and not done})
    return {
        "schema": 1, "since": since.isoformat(), "until": now.isoformat(),
        "throughput": completed,
        "lead_hours_p50": percentile(leads, .5),
        "lead_hours_p85": percentile(leads, .85),
        "wip_by_assignee": dict(wip), "ci": ci,
        "commits": len(commits),
        "revert_percent": ratio(reverts, len(commits)),
        "hatches": dict(hatches), "hatch_commits": hatch_commits,
        "hatch_commit_percent": ratio(hatch_commits, len(commits)),
        "false_positive_share": false_positives or None,
        "practice_reviews": reviews,
        "errors": errors,
        "warnings": (["Escape hatches exceed 10%; A8 follow-up is due"]
                     if hatch_commits > len(commits) / 10 else []),
    }
