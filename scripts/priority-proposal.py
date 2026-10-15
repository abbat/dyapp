"""A11: propose WSJF order locally; never change bead priorities."""
import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import json
import math
from pathlib import Path
import subprocess


def positive(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError("WSJF inputs must be numbers")
    if not math.isfinite(value) or value <= 0:
        raise ValueError("WSJF inputs must be finite and positive")
    return value


def proposal(beads, now):
    active = {b["id"]: b for b in beads if b["status"] != "closed"}
    children = {}
    for bead in beads:
        children.setdefault(bead.get("parent"), []).append(bead)

    def leaves(key, visiting):
        if key in visiting:
            raise ValueError("Cycle in bead parents")
        nested = children.get(key, [])
        if not nested:
            return int(key in active)
        return sum(leaves(b["id"], visiting | {key}) for b in nested)

    ranked = []
    for bead in active.values():
        if bead["issue_type"] != "epic" or "quorum" in bead.get("labels", []):
            continue
        rating = (bead.get("metadata") or {}).get("wsjf")
        if rating is None:
            delay = max(1, 5 - int(bead["priority"]))
            size = max(1, leaves(bead["id"], set()))
            reason = "Provisional proxies: 5-priority and remaining leaf count"
        else:
            delay = positive(rating["cost_of_delay"])
            size = positive(rating["job_size"])
            reason = rating["rationale"].strip()
            if not reason:
                raise ValueError("WSJF rationale is required")
        ranked.append({"id": bead["id"], "title": bead["title"],
                       "cost_of_delay": delay, "job_size": size,
                       "wsjf": delay / size, "provisional": rating is None,
                       "rationale": reason})
    ranked.sort(key=lambda b: (-b["wsjf"], b["id"]))
    digest = hashlib.sha256(json.dumps(
        ranked, sort_keys=True).encode()).hexdigest()
    return {"schema": 1, "generated_at": now, "proposal_sha256": digest,
            "order": ranked, "approval": "pending"}


def check(document, approval, now):
    generated = datetime.fromisoformat(document["generated_at"])
    current = datetime.fromisoformat(now)
    if generated.tzinfo is None or current.tzinfo is None:
        raise ValueError("Timestamps must include timezones")
    errors = []
    digest = hashlib.sha256(json.dumps(
        document["order"], sort_keys=True).encode()).hexdigest()
    if digest != document["proposal_sha256"]:
        errors.append("Proposal contents do not match its fingerprint")
    if not timedelta(0) <= current - generated <= timedelta(days=14):
        errors.append("Proposal is stale or from the future")
    if approval is None:
        errors.append("Human approval is missing")
    else:
        approved = datetime.fromisoformat(approval["approved_at"])
        if approved.tzinfo is None or not generated <= approved <= current:
            errors.append("Approval time is invalid")
        if not approval["approved_by"].strip():
            errors.append("Approver is missing")
        if approval["proposal_sha256"] != document["proposal_sha256"]:
            errors.append("Approval belongs to another proposal")
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, help="saved bd list JSON")
    parser.add_argument("--proposal", type=Path, help="saved proposal JSON")
    parser.add_argument("--approval", type=Path, help="human approval JSON")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    now = datetime.now(timezone.utc).isoformat()
    try:
        if args.proposal:
            document = json.loads(args.proposal.read_text())
        else:
            text = args.input.read_text() if args.input else \
                subprocess.check_output(
                    ["bd", "list", "--all", "--limit", "0",
                     "--brief", "--json"],
                    text=True)
            document = proposal(json.loads(text), now)
        approval = json.loads(args.approval.read_text()) \
            if args.approval else None
        errors = check(document, approval, now) if args.check else []
        document["errors"] = errors
        if args.check and not document["errors"]:
            document["approval"] = "approved"
    except (ValueError, KeyError, TypeError, OSError,
            subprocess.CalledProcessError) as error:
        document = {"errors": [str(error)]}
    text = json.dumps(document, indent=2) + "\n"
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(text)
    print(text, end="")
    return bool(document["errors"])


if __name__ == "__main__":
    raise SystemExit(main())
