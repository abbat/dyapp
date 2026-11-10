"""Convert cargo compiler-message JSON records to a valid SARIF document."""

import json
from pathlib import Path
import sys

results = []
seen: set[str] = set()
finished = None
for line in Path(sys.argv[1]).read_text().splitlines():
    item = json.loads(line)
    if item.get("reason") == "build-finished":
        finished = item["success"]
    if item.get("reason") != "compiler-message":
        continue
    message = item["message"]
    if message["level"] not in ("warning", "error"):
        continue
    result = {
        "ruleId": (message.get("code") or {}).get("code", "clippy"),
        "level": message["level"],
        "message": {"text": message["message"]},
    }
    spans = [span for span in message["spans"] if span["is_primary"]]
    if spans:
        span = spans[0]
        result["locations"] = [
            {
                "physicalLocation": {
                    "artifactLocation": {"uri": span["file_name"]},
                    "region": {
                        "startLine": max(1, span["line_start"]),
                        "startColumn": max(1, span["column_start"]),
                    },
                }
            }
        ]
    # Cargo reports the same source diagnostic for library and test targets.
    fingerprint = json.dumps(result, sort_keys=True)
    if fingerprint not in seen:
        seen.add(fingerprint)
        results.append(result)
if finished is not True:
    raise RuntimeError("Collector failed or did not finish")
print(
    json.dumps(
        {
            "version": "2.1.0",
            "runs": [
                {"tool": {"driver": {"name": "clippy"}}, "results": results}
            ],
        }
    )
)
