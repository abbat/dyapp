"""Execute the nonempty unit test inventory prepared by cargo test --no-run."""
import json
import subprocess
import sys

executables = set()
for line in open(sys.argv[1], encoding="utf-8"):
    item = json.loads(line)
    if item.get("reason") == "compiler-artifact" and item["profile"]["test"]:
        if item.get("executable"):
            executables.add(item["executable"])
if not executables:
    raise RuntimeError("No unit test binaries in build inventory")
for executable in sorted(executables):
    inventory = subprocess.run([executable, "--list"], check=True,
                               capture_output=True, text=True).stdout
    if not any(line.endswith(": test") for line in inventory.splitlines()):
        raise RuntimeError(f"Empty unit suite: {executable}")
    subprocess.run([executable], check=True)
