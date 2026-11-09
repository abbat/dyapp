"""Exercise the real pinned Kotlin check and correction entry point."""

from contextlib import contextmanager
from collections.abc import Iterator
from pathlib import Path
import shutil
import subprocess
from tempfile import TemporaryDirectory
import unittest

ROOT = Path(__file__).resolve().parents[1]


class KotlinFormatting(unittest.TestCase):
    @contextmanager
    def fixture(self) -> Iterator[Path]:
        build = ROOT / "build/kotlin-format-tests"
        build.mkdir(parents=True, exist_ok=True)
        with TemporaryDirectory(dir=build) as directory:
            root = Path(directory)
            scripts = root / "tests/scripts"
            scripts.mkdir(parents=True)
            shutil.copyfile(
                ROOT / "tests/scripts/kotlin-format.sh",
                scripts / "kotlin-format.sh",
            )
            for name in ("detekt.yml", "detekt-baseline.xml", ".editorconfig"):
                shutil.copyfile(ROOT / name, root / name)
            (root / "clients/android").mkdir(parents=True)
            yield root

    def run_check(
        self, root: Path, mode: str
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", str(root / "tests/scripts/kotlin-format.sh"), mode],
            cwd=root,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=120,
            check=False,
        )

    def test_rejects_and_fixes_both_kotlin_extensions(self) -> None:
        with self.fixture() as root:
            sources = root / "clients/android"
            kotlin = sources / "Answer.kt"
            script = sources / "Settings.kts"
            kotlin.write_text("internal fun answer(value:Int):Int=value\n")
            script.write_text("/** Fixture value. */\nval setting=1\n")
            generated = sources / "app/build/generated"
            generated.mkdir(parents=True)
            (generated / "Broken.kt").write_text("not valid Kotlin !!!\n")
            rejected = self.run_check(root, "lint")
            self.assertNotEqual(rejected.returncode, 0, rejected.stdout)
            self.assertIn("SpacingAround", rejected.stdout)
            self.assertEqual(
                kotlin.read_text(),
                "internal fun answer(value:Int):Int=value\n",
            )
            fixed = self.run_check(root, "format")
            self.assertEqual(fixed.returncode, 0, fixed.stdout)
            self.assertIn(
                "answer(value: Int): Int = value", kotlin.read_text()
            )
            self.assertIn("setting = 1", script.read_text())
            before = (kotlin.read_bytes(), script.read_bytes())
            clean = self.run_check(root, "lint")
            self.assertEqual(clean.returncode, 0, clean.stdout)
            self.assertEqual(
                before, (kotlin.read_bytes(), script.read_bytes())
            )

    def test_correction_keeps_complexity_failures_blocking(self) -> None:
        with self.fixture() as root:
            source = root / "clients/android/Branches.kt"
            branches = "".join(f"        {i} -> {i}\n" for i in range(12))
            source.write_text(
                "internal fun branch(value: Int): Int {\n"
                "    return when (value) {\n"
                + branches
                + "        else -> -1\n    }\n}\n"
            )
            result = self.run_check(root, "format")
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("[CyclomaticComplexMethod]", result.stdout)


if __name__ == "__main__":
    unittest.main()
