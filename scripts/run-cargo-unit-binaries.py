"""Execute the nonempty unit test inventory prepared by cargo test --no-run."""
import json
import subprocess
import sys

if __name__ == "__main__":
    from check_dispatch import wrap
    wrap("rust-test")

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
    listed = subprocess.run([executable, "--list"], check=False,
                            capture_output=True, text=True)
    if listed.returncode:
        print(listed.stdout, end="")
        print(listed.stderr, end="", file=sys.stderr)
        code = listed.returncode
        raise SystemExit(128 - code if code < 0 else code)
    inventory = listed.stdout
    if not any(line.endswith(": test") for line in inventory.splitlines()):
        raise RuntimeError(f"Empty unit suite: {executable}")
    status = subprocess.run([executable], check=False).returncode
    if status:
        raise SystemExit(128 - status if status < 0 else status)
