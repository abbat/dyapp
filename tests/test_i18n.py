"""D1: visible UI strings come from the platform catalogues."""
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "repository_i18n", ROOT / "scripts/check-repository.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
APPLE = json.dumps({"strings": {"status.ready": {}}})


class I18n(unittest.TestCase):
    def test_literals_and_unknown_keys_fail(self):
        texts = {
            CHECK.CATALOGUES["apple"]: APPLE,
            CHECK.CATALOGUES["desktop"]: '{"app.name": "DYApp"}',
            "android/app/src/main/kotlin/A.kt":
                'Text("Ready")\nText(stringResource(R.string.x))\n',
            "android/app/src/test/kotlin/T.kt": 'Text("test only")\n',
            "ios/DYApp/ContentView.swift":
                'Text("status.ready")\nText("Ready")\nText(peerId)\n',
            "desktop/ui/index.html":
                '<title data-i18n="app.name"></title><style>p{}</style>'
                '<p data-i18n="gone"></p><b>Hi</b><script>x()</script>',
        }
        self.assertEqual(CHECK.i18n_errors(texts), [
            "android/app/src/main/kotlin/A.kt: Text with a literal; "
            "use stringResource",
            "desktop/ui/index.html: literal text 'Hi'; use data-i18n",
            "desktop/ui/index.html: data-i18n key 'gone' not in "
            "desktop/ui/i18n/en.json",
            "ios/DYApp/ContentView.swift: Text key 'Ready' not in "
            "ios/DYApp/Localizable.xcstrings",
        ])

    def test_repository_ui_strings_come_from_catalogues(self):
        names = [*CHECK.CATALOGUES.values(),
                 "android/app/src/main/kotlin/com/dyapp/MainActivity.kt",
                 "ios/DYApp/ContentView.swift", "desktop/ui/index.html"]
        texts = {name: (ROOT / name).read_text() for name in names}
        self.assertEqual(CHECK.i18n_errors(texts), [])


if __name__ == "__main__":
    unittest.main()
