"""Require nonempty successful JUnit XML reports, without skipped tests."""
from pathlib import Path
import sys
import xml.etree.ElementTree as ET

reports = list(Path(sys.argv[1]).rglob("TEST-*.xml"))
if not reports:
    raise RuntimeError("Missing JUnit reports")
total = 0
for path in reports:
    root = ET.parse(path).getroot()
    suites = root.iter("testsuite")
    for suite in suites:
        total += int(suite.attrib["tests"])
        keys = ("failures", "errors", "skipped")
        if any(int(suite.attrib.get(key, 0)) for key in keys):
            raise RuntimeError(f"Failed or skipped tests: {path}")
if total == 0:
    raise RuntimeError("Empty JUnit inventory")
print(f"{total} JUnit tests passed")
