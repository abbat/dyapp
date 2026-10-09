"""Execute the nonempty unit test inventory prepared by cargo test --no-run."""
import json
import subprocess
import sys

executables = set()
with open(sys.argv[1], encoding="utf-8") as inventory_file:
    for line in inventory_file:
        item = json.loads(line)
        artifact = item.get("reason") == "compiler-artifact"
        if artifact and item["profile"]["test"] and item.get("executable"):
            executables.add(item["executable"])
if not executables:
    raise RuntimeError("No unit test binaries in build inventory")
for executable in sorted(executables):
    inventory = subprocess.run([executable, "--list"], check=True,
                               capture_output=True, text=True).stdout
    if not any(line.endswith(": test") for line in inventory.splitlines()):
        raise RuntimeError(f"Empty unit suite: {executable}")
    subprocess.run([executable], check=True)
