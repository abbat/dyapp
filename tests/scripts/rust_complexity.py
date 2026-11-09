"""Validate complete Clippy reports and enforce exact per-function limits."""

from collections import Counter
import re

LINTS = {
    "clippy::too_many_lines": "lines",
    "clippy::cognitive_complexity": "cognitive",
    "clippy::excessive_nesting": "nesting",
}


def diagnostics(metadata, records):
    """Check compilation completeness and retain workspace diagnostics."""
    members = set(metadata["workspace_members"])
    artifacts = {
        record["package_id"]
        for record in records
        if record.get("reason") == "compiler-artifact"
        if record["target"]["kind"] != ["custom-build"]
    }
    finished = [
        record
        for record in records
        if record.get("reason") == "build-finished"
    ]
    if not members or not members <= artifacts:
        raise ValueError("Missing compiled workspace members")
    if len(finished) != 1 or finished[0].get("success") is not True:
        raise ValueError("Missing unique successful Clippy completion")
    result = {}
    for record in records:
        if record.get("reason") != "compiler-message":
            continue
        message = record["message"]
        if message["level"] == "error":
            raise ValueError("Clippy error: " + message["message"])
        code = (message.get("code") or {}).get("code")
        if record["package_id"] not in members or code not in LINTS:
            continue
        spans = [span for span in message["spans"] if span["is_primary"]]
        if len(spans) != 1:
            raise ValueError("Expected one primary complexity span")
        span = spans[0]
        key = (code, span["file_name"], span["byte_start"], span["byte_end"])
        key += (message["message"],)
        result[key] = {
            "code": code,
            "span": span,
            "message": message["message"],
        }
    return list(result.values())


def offsets(source):
    """Translate proc-macro byte columns using the exact UTF-8 source."""
    result = [0]
    for line in source.splitlines(keepends=True):
        result.append(result[-1] + len(line))
    return result


def bounds(entry, lines, size):
    def byte(position):
        row, column = position
        if row < 1 or row >= len(lines) or column < 0:
            raise ValueError("Invalid parser source position")
        value = lines[row - 1] + column
        if value > lines[row] or value > size:
            raise ValueError("Parser column outside source")
        return value

    start, end = byte(entry["start"]), byte(entry["end"])
    if end <= start:
        raise ValueError("Empty or reversed function span")
    return start, end


def enclosing(finding, source, entries):
    span = finding["span"]
    start, end = span["byte_start"], span["byte_end"]
    if not 0 <= start < end <= len(source):
        raise ValueError("Diagnostic outside current source")
    lines, candidates = offsets(source), []
    for entry in entries:
        lo, hi = bounds(entry, lines, len(source))
        if lo <= start and end <= hi:
            if LINTS[finding["code"]] == "nesting":
                if entry["kind"] != "function":
                    continue
            candidates.append((hi - lo, lo, hi, entry))
    if not candidates:
        raise ValueError("No enclosing source function")
    candidates.sort(key=lambda item: item[:3])
    if len(candidates) > 1 and candidates[0][:3] == candidates[1][:3]:
        raise ValueError("Ambiguous enclosing function")
    return candidates[0][3]


def identity(entry, entries):
    if entry["kind"] != "function":
        raise ValueError("New complex anonymous closure")
    name = entry["id"]
    same = [
        item
        for item in entries
        if item["kind"] == "function" and item["id"] == name
    ]
    if len(same) != 1:
        raise ValueError("Ambiguous function identity")
    return name


def metric_value(finding, entry, metric):
    if metric == "nesting":
        return entry["nesting"]
    match = re.search(r"\((\d+)/(\d+)\)$", finding["message"])
    threshold = {"lines": 50, "cognitive": 15}[metric]
    if not match or int(match[2]) != threshold:
        raise ValueError("Unrecognized or overridden Clippy metric")
    return int(match[1])


def measure(findings, sources, functions):
    """Associate each finding with its innermost named function."""
    measured, regions, seen = {}, Counter(), set()
    for finding in findings:
        span = finding["span"]
        path = span["file_name"]
        diagnostic = (
            finding["code"],
            path,
            span["byte_start"],
            span["byte_end"],
            finding["message"],
        )
        if diagnostic in seen:
            continue
        seen.add(diagnostic)
        entries = functions[path]
        entry = enclosing(finding, sources[path], entries)
        key = (path, identity(entry, entries))
        values = measured.setdefault(key, {})
        metric = LINTS[finding["code"]]
        value = metric_value(finding, entry, metric)
        if metric == "nesting":
            regions[key] += 1
        values[metric] = max(values.get(metric, 0), value)
    for key, count in regions.items():
        measured[key]["nested_regions"] = count
    return measured


def growth_errors(current, previous):
    """A committed allowance can shrink, but cannot grow or change owners."""
    result = []
    for key, values in current.items():
        for metric, value in values.items():
            old = previous.get(key, {}).get(metric, 0)
            if value > old:
                result.append(
                    f"{key}: increased {metric} budget {old} -> {value}"
                )
    return result


def errors(measured, budgets):
    """Require explicit budget shrinkage and reject all new offenders."""
    result = []
    for key in sorted(measured.keys() | budgets.keys()):
        current, budget = measured.get(key, {}), budgets.get(key, {})
        for metric in sorted(current.keys() | budget.keys()):
            actual, allowed = current.get(metric, 0), budget.get(metric, 0)
            if actual > allowed:
                result.append(f"{key}: {metric} {actual} > {allowed}")
            elif actual < allowed:
                result.append(f"{key}: lower {metric} budget to {actual}")
    return result
