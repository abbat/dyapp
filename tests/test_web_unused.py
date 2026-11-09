"""Exercise dead-code detection through the real offline checker."""

import json
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "tools/web-lint/check-unused.mjs"


def fixture(root):
    sources = {
        "clients/desktop/ui/index.html":
            '<div class="used"></div><script type="module" '
            'src="entry.js"></script>',
        "clients/desktop/ui/entry.js": "document.body.textContent = 'ready';",
        "clients/desktop/ui/style.css": ".used { color: red; }",
        "clients/desktop/prototype/index.html": "<html></html>",
        "clients/desktop/ui-tests/mod.rs":
            'const SCRIPTS: &[&str] = &[include_str!("interface.js")];',
        "clients/desktop/ui-tests/interface.js": "",
        "tools/web-lint/runtime-selectors.json": '{"selectors": {}}',
    }
    for name, source in sources.items():
        destination = root / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(source)


class WebUnusedTest(unittest.TestCase):
    def test_real_checker_accepts_live_code_and_rejects_dead_code(self):
        cases = (
            ({}, 0, ""),
            ({"clients/desktop/ui/extra.js": "export const unused = 1;"},
             1, "extra.js"),
            ({"clients/desktop/ui/entry.js": "export const unused = 1;"},
             1, "unused"),
            ({"clients/desktop/ui/style.css": ".ghost { color: red; }"},
             1, "unreferenced selector .ghost"),
            ({"clients/desktop/ui/package.json": json.dumps({
                "name": "fixture", "private": True,
                "dependencies": {"unused-package": "1.0.0"}})},
             1, "unused-package"),
            ({"clients/desktop/ui/entry.js": "import 'missing-package';"},
             1, "missing-package"),
            ({"clients/desktop/ui/entry.js":
              "document.body.classList.add('generated');",
              "clients/desktop/ui/style.css": ".generated { color: red; }"},
             0, ""),
            ({"clients/desktop/ui/style.css": ".runtime-ready { color: red; }",
              "tools/web-lint/runtime-selectors.json": json.dumps({
                  "selectors": {".runtime-ready": "native state suffix"}})},
             0, ""),
            ({"tools/web-lint/runtime-selectors.json": json.dumps({
                "selectors": {".stale": "removed runtime path"}})},
             1, "stale runtime selector"),
            ({"clients/desktop/ui/index.html": "<div></div>"},
             2, "No desktop module entries"),
            ({"clients/desktop/ui-tests/mod.rs":
              "const SCRIPTS: &[&str] = &[];"},
             2, "Empty or duplicated"),
            ({"clients/desktop/ui/entry.js": "const = broken;"},
             2, "Web unused-code check failed"),
        )
        for changes, expected, message in cases:
            with self.subTest(changes=changes):
                with tempfile.TemporaryDirectory(dir=ROOT / "build") as tmp:
                    root = Path(tmp)
                    fixture(root)
                    for name, source in changes.items():
                        (root / name).write_text(source)
                    result = subprocess.run(
                        ["node", str(CHECK), str(root)], cwd=ROOT,
                        capture_output=True, text=True, check=False)
                output = result.stdout + result.stderr
                self.assertEqual(result.returncode, expected, output)
                self.assertIn(message, output)


if __name__ == "__main__":
    unittest.main()
