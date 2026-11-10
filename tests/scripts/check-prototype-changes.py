"""Select the desktop prototype matrix from the actual Git change range."""

import os
from pathlib import Path
import subprocess

PATHS = (
    "clients/desktop/ui/",
    "clients/desktop/prototype/",
    "clients/desktop/ui-tests/",
    "tests/scripts/",
    "tests/docker/",
    "tools/scripts/",
    "tools/web-lint/",
    ".github/workflows/",
    "docs/design/review-criteria.md",
    "tests/test_prototype_changes.py",
    "tests/test_final_check_output.py",
)


def needed(event: str, base: str, head: str) -> bool:
    if event == "workflow_dispatch":
        return True
    if event not in ("push", "pull_request"):
        raise ValueError(f"Unsupported CI event: {event}")
    if not base or set(base) == {"0"}:
        return True
    if event == "pull_request":
        base = subprocess.check_output(
            ["git", "merge-base", base, head], text=True
        ).strip()
    result = subprocess.run(
        ["git", "diff", "--quiet", base, head, "--", *PATHS], check=False
    )
    if result.returncode not in (0, 1):
        result.check_returncode()
    return result.returncode == 1


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("prototype-changes")
    selected = needed(
        os.environ["EVENT_NAME"],
        os.environ.get("DIFF_BASE", ""),
        os.environ["DIFF_HEAD"],
    )
    with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
        output.write(f"needed={str(selected).lower()}\n")
    print(f"Desktop prototype matrix needed: {selected}")
