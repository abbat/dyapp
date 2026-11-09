"""Check coverage forwarding and exact staged Python with real flake8."""

from contextlib import redirect_stdout
import importlib.util
import io
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).parent / "scripts" / "check-fast-precommit.py"
SPEC = importlib.util.spec_from_file_location("check_fast_precommit", SCRIPT)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class FastPrecommit(unittest.TestCase):
    def test_cleared_environment_requires_coverage_path(self) -> None:
        source = """let output = Command::new("dyappd")
    .env_clear()
    .env("LANG", "C")
    .output();"""
        self.assertEqual(CHECK.missing_profile_lines(source), [2])

    def test_forwarded_coverage_path_passes(self) -> None:
        source = """let output = Command::new("dyappd")
    .env_clear()
    .envs(std::env::var_os("LLVM_PROFILE_FILE")
        .map(|p| ("LLVM_PROFILE_FILE", p)))
    .output();"""
        self.assertEqual(CHECK.missing_profile_lines(source), [])


def source(branches: int) -> str:
    lines = ["def classify(value):"]
    for index in range(branches):
        lines.extend((f"    if value == {index}:", f"        return {index}"))
    return "\n".join([*lines, "    return -1", ""])


def git(root: Path, *args: str) -> bytes:
    return subprocess.check_output(["git", "-C", str(root), *args])


def local_lint(directory: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        CHECK.python_command(directory),
        capture_output=True,
        text=True,
        check=False,
    )


class StagedPython(unittest.TestCase):
    def check_snapshot(
        self,
        root: Path,
        relative: str,
        staged: str,
        working: str,
    ) -> tuple[bool, str]:
        git(root, "init", "--quiet")
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(staged)
        git(root, "add", "--", relative)
        path.write_text(working)
        output = io.StringIO()
        with (
            patch.object(
                CHECK, "git", side_effect=lambda *args: git(root, *args)
            ),
            patch.object(CHECK, "python_lint", side_effect=local_lint) as lint,
            redirect_stdout(output),
        ):
            result = CHECK.check_python([relative])
        self.assertEqual(lint.call_count, 1)
        self.assertEqual(path.read_text(), working)
        self.assertEqual(git(root, "show", ":" + relative).decode(), staged)
        return result, output.getvalue()

    def test_staged_complexity_fails_despite_clean_working_copy(self) -> None:
        with TemporaryDirectory() as directory:
            accepted, text = self.check_snapshot(
                Path(directory), "tools/with spaces.py", source(10), source(9)
            )
        self.assertFalse(accepted)
        self.assertIn("C901", text)
        self.assertIn("tools/with spaces.py:1:", text)
        self.assertIn("FIX: docs/development/linting.md#python", text)

    def test_staged_boundary_passes_despite_bad_working_copy(self) -> None:
        with TemporaryDirectory() as directory:
            accepted, text = self.check_snapshot(
                Path(directory), "tools/fixture.py", source(9), source(10)
            )
        self.assertTrue(accepted, text)

    def test_noqa_cannot_hide_staged_complexity(self) -> None:
        staged = source(10).replace(
            "def classify(value):", "def classify(value):  # noqa: C901"
        )
        with TemporaryDirectory() as directory:
            accepted, text = self.check_snapshot(
                Path(directory), "tools/fixture.py", staged, source(9)
            )
        self.assertFalse(accepted)
        self.assertIn("C901", text)

    def test_syntax_errors_fail_instead_of_skipping_measurement(self) -> None:
        with TemporaryDirectory() as directory:
            accepted, text = self.check_snapshot(
                Path(directory),
                "tools/fixture.py",
                "def broken(:\n",
                source(9),
            )
        self.assertFalse(accepted)
        self.assertIn("E9", text)

    def test_multiple_python_files_share_one_linter_invocation(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            git(root, "init", "--quiet")
            sources = {"tools/good.py": source(9), "tests/bad.py": source(10)}
            for name, content in sources.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
                git(root, "add", "--", name)
            output = io.StringIO()
            with (
                patch.object(
                    CHECK, "git", side_effect=lambda *args: git(root, *args)
                ),
                patch.object(
                    CHECK, "python_lint", side_effect=local_lint
                ) as lint,
                redirect_stdout(output),
            ):
                self.assertFalse(CHECK.check_python(list(sources)))
            self.assertEqual(lint.call_count, 1)
            self.assertIn("tests/bad.py:1:", output.getvalue())

    def test_non_python_changes_need_no_docker_or_index_reads(self) -> None:
        with (
            patch.object(CHECK, "git") as staged,
            patch.object(CHECK, "python_lint") as lint,
        ):
            self.assertTrue(CHECK.check_python(["rust/a.rs", "README.md"]))
        staged.assert_not_called()
        lint.assert_not_called()


if __name__ == "__main__":
    unittest.main()
