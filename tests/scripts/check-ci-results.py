"""Require every expected platform CI job to complete successfully."""

import argparse
import json
import os

REQUIRED = {
    "core",
    "docker",
    "linux",
    "apple",
    "windows",
    "prototype_changes",
    "prototype",
}


def intentional_skip(name: str, value: object, selection: object) -> bool:
    if (
        name != "prototype"
        or not isinstance(value, dict)
        or value.get("result") != "skipped"
    ):
        return False
    if not isinstance(selection, dict) or selection.get("result") != "success":
        return False
    outputs = selection.get("outputs")
    return isinstance(outputs, dict) and outputs.get("needed") == "false"


def validate(results: object) -> None:
    if not isinstance(results, dict) or not results:
        raise ValueError("Missing or invalid CI job inventory")
    errors = [f"{name}: missing" for name in sorted(REQUIRED - results.keys())]
    selection = results.get("prototype_changes")
    for name, value in sorted(results.items()):
        if intentional_skip(name, value, selection):
            continue
        if not isinstance(value, dict) or value.get("result") != "success":
            state = value.get("result") if isinstance(value, dict) else value
            errors.append(f"{name}: {state!r}")
    if errors:
        raise ValueError("CI jobs not successful: " + "; ".join(errors))


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("ci-results")
    argparse.ArgumentParser(description=__doc__).parse_args()
    validate(json.loads(os.environ["RESULTS"]))
    print("All required CI jobs passed")
