"""am instrument may exit zero on test failures; require the JUnit result."""
import json
from pathlib import Path
import re
import sys

output = Path(sys.argv[1]).read_text()
print(output)
match = re.search(r"^OK \((\d+) tests?\)$", output, re.MULTILINE)
if not match or int(match.group(1)) < 3 or "FAILURES!!!" in output:
    raise RuntimeError("Missing, incomplete, or failed Android UI suite")
last_line = output.strip().splitlines()[-1]
if "INSTRUMENTATION_" in output:
    if last_line != "INSTRUMENTATION_CODE: -1":
        raise RuntimeError("Instrumentation did not finish successfully")
elif last_line != match.group(0):
    raise RuntimeError("Incomplete or failed human-readable instrumentation")
Path(sys.argv[1]).with_suffix(".json").write_text(
    json.dumps({"platform": "android", "tests": int(match.group(1)),
                "passed": True}, indent=2) + "\n")
