"""Practice C3: the OpenSSF Scorecard score never drops below its baseline.

Reads the JSON result of scorecard-action. Below 7.0 (the practice's target)
it also lists the checks that lose the most points.
"""
import json
from pathlib import Path
import sys
import tomllib

BASELINES = Path(__file__).with_name("baselines.toml")
TARGET = 7.0


def check(result, baseline):
    """The report; fails below the baseline."""
    score = result["score"]
    if score < baseline:
        raise ValueError(f"Scorecard {score} < baseline {baseline} "
                         "(tests/scripts/baselines.toml)")
    lines = [f"Scorecard {score} (baseline {baseline}, target {TARGET})"]
    if score >= baseline + 0.5:
        lines.append("Raise [scorecard] score in "
                     "tests/scripts/baselines.toml to "
                     f"{score}")
    if score < TARGET:
        weak = sorted((c for c in result["checks"] if 0 <= c["score"] < 10),
                      key=lambda c: c["score"])
        lines += [f"  {c['name']}: {c['score']} - {c['reason']}"
                  for c in weak]
    return "\n".join(lines)


if __name__ == "__main__":
    from check_wrapper import wrap
    wrap("scorecard")
    result = json.loads(Path(sys.argv[1]).read_text())
    baseline = tomllib.loads(BASELINES.read_text())["scorecard"]["score"]
    print(check(result, baseline))
