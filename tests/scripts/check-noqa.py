"""Fail on a Python `# noqa` without codes and a reason, or one flake8 no
longer needs: flake8 has no unused-noqa check, so rerun it with
`--disable-noqa` and require every suppressed code on its line."""
import io
from pathlib import Path
import re
import subprocess
import sys
import tokenize

ROOT = Path(__file__).resolve().parents[2]
NOQA = re.compile(r"#\s*noqa\b(.*)", re.I)
FORM = re.compile(r":\s*([A-Z]+\d+(?:,[A-Z]+\d+)*)\s+--\s+\S")
REPORT = re.compile(r"^(.+):(\d+):\d+: ([A-Z]+\d+)\b")


def suppressions(text):
    """Yield (line, codes or None) for each noqa comment; None is
    malformed. Strings that mention noqa are not comments."""
    for token in tokenize.generate_tokens(io.StringIO(text).readline):
        if token.type == tokenize.COMMENT and (
                match := NOQA.search(token.string)):
            form = FORM.match(match.group(1))
            yield token.start[0], form.group(1).split(",") if form else None


def errors(sources, reported):
    """Problems in {path: text} given flake8 (path, line, code) findings."""
    result = []
    for path, text in sorted(sources.items()):
        for line, codes in suppressions(text):
            if codes is None:
                result.append(
                    f"{path}:{line}: write `# noqa: CODE -- reason`")
                continue
            for code in codes:
                if (path, line, code) not in reported:
                    result.append(f"{path}:{line}: unused noqa {code}")
    return result


def flake8(paths):
    """Findings flake8 reports with every noqa ignored."""
    run = subprocess.run(
        [sys.executable, "-m", "flake8", "--disable-noqa", "--exit-zero",
         *paths], cwd=ROOT, capture_output=True, text=True, check=False)
    if run.returncode:
        raise SystemExit(run.stderr or "flake8 failed")
    return {(m[1], int(m[2]), m[3]) for line in run.stdout.splitlines()
            if (m := REPORT.match(line))}


def main(dirs):
    sources = {}
    for directory in dirs:
        for path in sorted((ROOT / directory).rglob("*.py")):
            text = path.read_text(encoding="utf-8")
            if any(suppressions(text)):
                sources[str(path.relative_to(ROOT))] = text
    result = errors(sources, flake8(sources) if sources else set())
    print("\n".join(result) or f"{len(sources)} files with noqa checked")
    return 1 if result else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:] or ["tests", "tools"]))
