"""The fast hook must catch coverage subprocesses without running Docker."""

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).parent / "scripts" / "check-fast-precommit.py"
SPEC = importlib.util.spec_from_file_location("check_fast_precommit", SCRIPT)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class FastPrecommit(unittest.TestCase):
    def test_cleared_environment_requires_coverage_path(self):
        source = '''let output = Command::new("dyappd")
    .env_clear()
    .env("LANG", "C")
    .output();'''
        self.assertEqual(CHECK.missing_profile_lines(source), [2])

    def test_forwarded_coverage_path_passes(self):
        source = '''let output = Command::new("dyappd")
    .env_clear()
    .envs(std::env::var_os("LLVM_PROFILE_FILE")
        .map(|p| ("LLVM_PROFILE_FILE", p)))
    .output();'''
        self.assertEqual(CHECK.missing_profile_lines(source), [])


if __name__ == "__main__":
    unittest.main()
