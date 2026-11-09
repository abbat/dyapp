"""Compile actual smoke includes and locale queries for both feature modes."""
from pathlib import Path
import re
import subprocess
import time
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]


def function(source, name):
    start = source.index("fn " + name + "(")
    return source[start:source.index("\n}", start) + 2]


class DesktopSmokeFeatureTest(unittest.TestCase):
    def directory(self, name):
        path = ROOT / "build/tool-fixtures/desktop-smoke-feature"
        path = path / str(time.time_ns()) / name
        path.mkdir(parents=True)
        return path

    def compile(self, directory, source, enabled):
        path = directory / "probe.rs"
        path.write_text(source)
        binary = directory / "probe"
        args = ["rustc", "--edition=2021", "-D", "warnings", str(path),
                "-o", str(binary)]
        if enabled:
            args.extend(["--cfg", 'feature="ui-smoke"'])
        subprocess.run(args, check=True, capture_output=True, text=True)
        return binary

    def test_feature_is_explicit_in_both_native_manifests(self):
        for platform in ("linux", "windows"):
            manifest = ROOT / "clients" / platform / "Cargo.toml"
            features = tomllib.loads(manifest.read_text())["features"]
            self.assertEqual(features["ui-smoke"], [])
            self.assertNotIn("ui-smoke", features["default"])

    def test_actual_module_gate_omits_script_bytes_without_feature(self):
        main = (ROOT / "clients/desktop/main.rs").read_text()
        gate = re.search(r'#\[cfg\(feature = "ui-smoke"\)\]\n'
                         r'#\[path = "ui-tests/mod.rs"\]\nmod ui_smoke;',
                         main)
        self.assertIsNotNone(gate)
        directory = self.directory("embedding")
        bundle = directory / "bundle.rs"
        source_dir = ROOT / "clients/desktop/ui-tests"
        module = (source_dir / "mod.rs").read_text()
        source = module[module.index("const SCRIPTS:"):
                        module.index("/// Enable the isolated")]
        source = re.sub(
            r'include_str!\("([^"]+)"\)',
            lambda match: 'include_str!("{}")'.format(
                (source_dir / match[1]).resolve().as_posix()), source)
        bundle.write_text(source)
        declaration = gate[0].replace("ui-tests/mod.rs", bundle.as_posix())
        probe = declaration + '\nfn main() {\n' + (
            '    #[cfg(feature = "ui-smoke")]\n'
            '    print!("{}", ui_smoke::scripts());\n'
            '    #[cfg(not(feature = "ui-smoke"))]\n'
            '    print!("ordinary");\n}\n')
        for enabled in (False, True):
            case = directory / str(enabled)
            case.mkdir()
            binary = self.compile(case, probe, enabled)
            result = subprocess.check_output([str(binary)])
            if enabled:
                self.assertTrue(result.startswith(b"(async () => {"))
                self.assertIn(b"const appApi = {", result)
                self.assertIn(b"const appBindings = {", result)
                self.assertNotIn(b"window.identityShown", result)
                self.assertIn(b"ui_test_result", result)
                self.assertIn(b"ui_test_result", binary.read_bytes())
            else:
                self.assertEqual(result, b"ordinary")
                self.assertNotIn(b"ui_test_result", binary.read_bytes())

    def test_actual_locale_query_ignores_smoke_flags_without_feature(self):
        main = (ROOT / "clients/desktop/main.rs").read_text()
        source = function(main, "locale_options") + '\n'
        source += '#[cfg(feature = "ui-smoke")]\n'
        source += function(main, "ui_test_russian")
        source += '\nfn main() { println!("{:?}", locale_options()); }\n'
        for enabled in (False, True):
            binary = self.compile(
                self.directory(str(enabled)), source, enabled)
            for args, expected in (([], "[false, false]"),
                                   (["--pseudo-locale"], "[true, false]"),
                                   (["--ui-test-ru"], "[false, false]"),
                                   (["--ui-test", "--ui-test-ru"],
                                    f"[false, {str(enabled).lower()}]")):
                result = subprocess.check_output(
                    [str(binary), *args], text=True)
                self.assertEqual(result.strip(), expected)


if __name__ == "__main__":
    unittest.main()
