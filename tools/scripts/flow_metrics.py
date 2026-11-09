"""Pure A7 calculations over a local, versioned snapshot schema."""

from collections import Counter
from datetime import datetime, timedelta, timezone
import math

from collections.abc import Sequence
from typing import NotRequired, Required, TypedDict, cast


class Job(TypedDict):
    started_at: str | None
    completed_at: str | None


class Run(TypedDict):
    id: NotRequired[int]
    head_sha: str
    created_at: str
    completed_at: str | None
    conclusion: str | None
    attempt: int
    jobs: Sequence[Job]
    workflow: str


class Bead(TypedDict, total=False):
    status: Required[str]
    issue_type: Required[str]
    id: str
    assignee: str | None
    external_ref: str | None
    created_at: str
    closed_at: str


class Commit(TypedDict):
    sha: NotRequired[str]
    at: str
    message: str
    trailers: str
    annotations: NotRequired[dict[str, int]]


class Practice(TypedDict):
    id: str
    adopted_at: str


class Review(TypedDict):
    practice: str
    at: str


class Observation(TypedDict):
    check: str
    at: str
    false_positive: bool | str


class Alert(TypedDict):
    ghsa_id: str
    created_at: str


class Sources(TypedDict):
    repository: str
    main: str
    github_runs: int
    selected_runs: int


class Snapshot(TypedDict, total=False):
    schema: Required[int]
    now: Required[str]
    runs: Required[list[Run]]
    sources: Sources
    beads: list[Bead]
    commits: list[Commit]
    practices: list[Practice]
    reviews: list[Review]
    observations: list[Observation]
    alerts: list[Alert]


class RedInterval(TypedDict):
    started_at: str
    ended_at: str


class CIMetrics(TypedDict):
    main_state: str
    red_commit_percent: float | None
    failed_commits: int
    completed_commits: int
    max_red_hours: float
    red_intervals: list[RedInterval]
    current_red_hours: float
    time_to_green_hours_p50: float | None
    time_to_green_hours_p85: float | None
    recovered_intervals: int
    job_minutes: float
    pending_runs: int


class Postmortem(TypedDict):
    trigger: str
    at: str
    due_at: str
    bead: str | Bead | None
    late: bool


class Health(TypedDict):
    schema: int
    until: str
    sources: Sources | None
    main_state: str
    current_red_hours: float
    red_intervals: list[RedInterval]
    errors: list[str]
    warnings: list[str]


class FalsePositives(TypedDict):
    samples: int
    false_positives: int
    percent: float | None


class PracticeReview(TypedDict):
    practice: str
    due_at: str
    reviewed: bool
    overdue: bool


class Report(TypedDict):
    schema: int
    since: str
    until: str
    sources: Sources | None
    throughput: int
    lead_hours_p50: float | None
    lead_hours_p85: float | None
    wip_by_assignee: dict[str, int]
    ci: CIMetrics
    commits: int
    revert_percent: float | None
    hatches: dict[str, int]
    hatch_commits: int
    hatch_commit_percent: float | None
    false_positive_share: dict[str, FalsePositives] | None
    practice_reviews: list[PracticeReview]
    postmortems: list[Postmortem]
    errors: list[str]
    warnings: list[str]


RunOrder = tuple[datetime, int, int]
RedHistory = tuple[
    datetime | None,
    list[tuple[datetime, datetime]],
    list[float],
    dict[str, set[str | None]],
]

HATCHES = {"Large-Change", "Size-Increase", "Docs-Unchanged", "Quorum"}
BAD = {"failure", "timed_out", "startup_failure", "action_required", "stale"}


def instant(value: str) -> datetime:
    result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if result.tzinfo is None:
        raise ValueError("Timestamps must include a timezone")
    return result.astimezone(timezone.utc)


def percentile(values: Sequence[float], fraction: float) -> float | None:
    return (
        sorted(values)[math.ceil(len(values) * fraction) - 1]
        if values
        else None
    )


def ratio(count: int, total: int) -> float | None:
    return 100 * count / total if total else None


def run_order(run: Run) -> RunOrder:
    return instant(run["created_at"]), run.get("id", 0), run["attempt"]


def job_minutes(runs: Sequence[Run], since: datetime, now: datetime) -> float:
    minutes: float = 0
    for run in runs:
        for job in run["jobs"]:
            if not job.get("started_at"):
                continue
            start = instant(cast(str, job["started_at"]))
            end = (
                instant(cast(str, job["completed_at"]))
                if job.get("completed_at")
                else now
            )
            if end < start:
                raise ValueError("Job completion precedes start")
            clipped_start, clipped_end = max(start, since), min(end, now)
            elapsed = (clipped_end - clipped_start).total_seconds()
            minutes += max(0, elapsed) / 60
    return minutes


def red_history(
    ci: Sequence[Run], since: datetime, now: datetime
) -> RedHistory:
    events = [r for r in ci if r["completed_at"]]
    events.sort(key=lambda r: instant(cast(str, r["completed_at"])))
    red: datetime | None = None
    generation: RunOrder | None = None
    green_generation: RunOrder | None = None
    intervals: list[tuple[datetime, datetime]] = []
    recovery: list[float] = []
    outcomes: dict[str, set[str | None]] = {}
    for run in events:
        end = instant(cast(str, run["completed_at"]))
        if end > now:
            continue
        order = run_order(run)
        conclusion = run["conclusion"]
        if since <= end < now and conclusion in BAD | {"success"}:
            outcomes.setdefault(run["head_sha"], set()).add(conclusion)
        if conclusion == "success":
            green_generation = max(green_generation or order, order)
            if red is not None and order >= cast(RunOrder, generation):
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
    return red, intervals, recovery, outcomes


def main_state(ci: Sequence[Run], red: datetime | None, now: datetime) -> str:
    latest = max(ci, key=run_order)
    state = "unknown"
    if red is not None:
        state = "red"
    elif latest["completed_at"]:
        if instant(latest["completed_at"]) <= now:
            if latest["conclusion"] == "success":
                state = "green"
    return state


def ci_metrics(
    runs: Sequence[Run], since: datetime, now: datetime
) -> CIMetrics:
    ci = [r for r in runs if r["workflow"] == ".github/workflows/ci.yml"]
    if not ci:
        raise ValueError("No canonical main CI history")
    red, intervals, recovery, outcomes = red_history(ci, since, now)
    red_hours = [
        (end - start).total_seconds() / 3600
        for start, end in intervals
        if end >= since
    ]
    minutes = job_minutes(runs, since, now)
    failed = sum(bool(results & BAD) for results in outcomes.values())
    state = main_state(ci, red, now)
    return {
        "main_state": state,
        "red_commit_percent": ratio(failed, len(outcomes)),
        "failed_commits": failed,
        "completed_commits": len(outcomes),
        "max_red_hours": max(red_hours, default=0),
        "red_intervals": [
            {"started_at": start.isoformat(), "ended_at": end.isoformat()}
            for start, end in intervals
            if end >= since
        ],
        "current_red_hours": (now - red).total_seconds() / 3600
        if red is not None
        else 0,
        "time_to_green_hours_p50": percentile(recovery, 0.5),
        "time_to_green_hours_p85": percentile(recovery, 0.85),
        "recovered_intervals": len(recovery),
        "job_minutes": minutes,
        "pending_runs": sum(not r["completed_at"] for r in runs),
    }


def postmortems(
    snapshot: Snapshot, ci: CIMetrics, now: datetime
) -> tuple[list[Postmortem], list[str]]:
    """A8: each trigger after adoption needs a `postmortem:<key>` bead
    created within 48 hours of the trigger."""
    adopted = [
        instant(p["adopted_at"])
        for p in snapshot["practices"]
        if p["id"] == "A8"
    ]
    if not adopted:
        return [], []
    day = timedelta(hours=24)
    triggers = [
        (f"ci-red:{red['started_at']}", instant(red["started_at"]) + day)
        for red in ci["red_intervals"]
        if instant(red["started_at"]) + day < instant(red["ended_at"])
    ]
    triggers += [
        (f"advisory:{alert['ghsa_id']}", instant(alert["created_at"]))
        for alert in snapshot.get("alerts", [])
    ]
    beads = {b.get("external_ref"): b for b in snapshot["beads"]}
    rows: list[Postmortem] = []
    errors: list[str] = []
    first: dict[str, datetime] = {}
    for key, at in triggers:  # one advisory may raise several alerts
        first[key] = min(at, first.get(key, at))
    for key, at in sorted(first.items(), key=lambda trigger: trigger[1]):
        if at < adopted[0]:
            continue
        due = at + timedelta(hours=48)
        bead = beads.get(f"postmortem:{key}")
        late = instant(bead["created_at"]) > due if bead else due <= now
        rows.append(
            {
                "trigger": key,
                "at": at.isoformat(),
                "due_at": due.isoformat(),
                "bead": bead and bead["id"],
                "late": late,
            }
        )
        if late:
            errors.append(
                f"Postmortem for {key} missing or later than {due.isoformat()}"
            )
    return rows, errors


def health(snapshot: Snapshot) -> Health:
    """Daily canonical-main check: red for over 24 hours is an error; red for
    less, or an unresolved latest run, is a warning; only green is clean."""
    if snapshot["schema"] != 1:
        raise ValueError("Unsupported snapshot schema")
    now = instant(snapshot["now"])
    ci = ci_metrics(snapshot["runs"], now - timedelta(days=7), now)
    errors, warnings = [], []
    if ci["current_red_hours"] > 24:
        errors.append(f"Main red {ci['current_red_hours']:.2f} hours > 24")
    elif ci["main_state"] == "red":
        warnings.append(
            f"Main red {ci['current_red_hours']:.2f} hours; fix before 24"
        )
    elif ci["main_state"] == "unknown":
        warnings.append("Main state unknown: latest run is not a success")
    return {
        "schema": 1,
        "until": now.isoformat(),
        "sources": snapshot.get("sources"),
        "main_state": ci["main_state"],
        "current_red_hours": ci["current_red_hours"],
        "red_intervals": ci["red_intervals"],
        "errors": errors,
        "warnings": warnings,
    }


def bead_metrics(
    snapshot: Snapshot, since: datetime, now: datetime
) -> tuple[list[str], list[float], Counter[str], int]:
    def within(value: str) -> bool:
        return since <= instant(value) < now

    errors: list[str] = []
    leads: list[float] = []
    wip: Counter[str] = Counter()
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
    errors += [
        f"WIP {agent}: {count} > 3"
        for agent, count in wip.items()
        if count > 3
    ]
    return errors, leads, wip, completed


def commit_metrics(commits: Sequence[Commit]) -> tuple[Counter[str], int, int]:
    hatches: Counter[str] = Counter()
    hatch_commits, reverts = 0, 0
    for commit in commits:
        used: Counter[str] = Counter()
        for line in commit["trailers"].splitlines():
            key, sep, value = line.partition(":")
            if sep and key in HATCHES and value.strip():
                used[key] += 1
        used.update(commit.get("annotations", {}))
        hatches.update(used)
        hatch_commits += bool(used)
        reverts += "This reverts commit " in commit["message"]
    return hatches, hatch_commits, reverts


def false_positive_metrics(
    snapshot: Snapshot, since: datetime, now: datetime
) -> dict[str, FalsePositives]:
    def within(value: str) -> bool:
        return since <= instant(value) < now

    checks: dict[str, list[int]] = {}
    for sample in snapshot["observations"]:
        if not isinstance(sample["false_positive"], bool):
            raise ValueError("false_positive must be a JSON boolean")
        if within(sample["at"]):
            counts = checks.setdefault(sample["check"], [0, 0])
            counts[0] += 1
            counts[1] += sample["false_positive"]
    false_positives: dict[str, FalsePositives] = {
        name: {
            "samples": total,
            "false_positives": false,
            "percent": ratio(false, total),
        }
        for name, (total, false) in checks.items()
    }
    return false_positives


def report(snapshot: Snapshot) -> Report:
    if snapshot["schema"] != 1:
        raise ValueError("Unsupported snapshot schema")
    now = instant(snapshot["now"])
    since = now - timedelta(days=7)

    def within(value: str) -> bool:
        return since <= instant(value) < now

    errors, leads, wip, completed = bead_metrics(snapshot, since, now)
    ci = ci_metrics(snapshot["runs"], since, now)
    if ci["max_red_hours"] > 24:
        errors.append(f"Main red {ci['max_red_hours']:.2f} hours > 24")
    reviewed, late = postmortems(snapshot, ci, now)
    errors += late
    commits = [c for c in snapshot["commits"] if within(c["at"])]
    hatches, hatch_commits, reverts = commit_metrics(commits)
    false_positives = false_positive_metrics(snapshot, since, now)
    reviews: list[PracticeReview] = []
    for practice in snapshot["practices"]:
        adopted = instant(practice["adopted_at"])
        due = adopted + timedelta(days=28)
        done = [
            r
            for r in snapshot["reviews"]
            if all(
                (
                    r["practice"] == practice["id"],
                    due <= instant(r["at"]) <= now,
                )
            )
        ]
        reviews.append(
            {
                "practice": practice["id"],
                "due_at": due.isoformat(),
                "reviewed": bool(done),
                "overdue": due <= now and not done,
            }
        )
    return {
        "schema": 1,
        "since": since.isoformat(),
        "until": now.isoformat(),
        "sources": snapshot.get("sources"),
        "throughput": completed,
        "lead_hours_p50": percentile(leads, 0.5),
        "lead_hours_p85": percentile(leads, 0.85),
        "wip_by_assignee": dict(wip),
        "ci": ci,
        "commits": len(commits),
        "revert_percent": ratio(reverts, len(commits)),
        "hatches": dict(hatches),
        "hatch_commits": hatch_commits,
        "hatch_commit_percent": ratio(hatch_commits, len(commits)),
        "false_positive_share": false_positives or None,
        "practice_reviews": reviews,
        "postmortems": reviewed,
        "errors": errors,
        "warnings": (
            ["Escape hatches exceed 10%; A8 follow-up is due"]
            if hatch_commits > len(commits) / 10
            else []
        ),
    }
