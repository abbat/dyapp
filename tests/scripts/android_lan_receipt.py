"""Validate the owned device's completed route/identity checkpoint."""

import json
from pathlib import Path
import sys


def validate(value: object, phase: str, node: str, public_node: str) -> None:
    if not isinstance(value, dict):
        raise ValueError("Missing LAN permission receipt object")
    expected = {"phase": phase, "node": node, "publicNode": public_node}
    for key, wanted in expected.items():
        if value.get(key) != wanted:
            raise ValueError(f"LAN receipt mismatch: {key}")
    for key in ("owner", "nodePeer", "process"):
        field = value.get(key)
        if not isinstance(field, str) or not field.strip():
            raise ValueError(f"Missing LAN receipt identity: {key}")
    for key in ("sdk", "target"):
        field = value.get(key)
        if not isinstance(field, int) or field < 37:
            raise ValueError(f"LAN receipt is not a modern device: {key}")


def main() -> int:
    path, phase, node, public_node = sys.argv[1:]
    with Path(path).open("rb") as source:
        data = source.read(8193)
    if len(data) > 8192:
        raise ValueError("Oversized LAN permission receipt")
    validate(json.loads(data), phase, node, public_node)
    print(f"LAN permission receipt: {phase} verified")
    return 0


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("ui-reports")
    raise SystemExit(main())
