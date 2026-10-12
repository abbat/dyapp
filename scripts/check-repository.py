"""Nonmutating equivalents of basic repository hooks, run inside Docker."""
import json
from pathlib import Path, PurePosixPath
import posixpath
import re
import subprocess
from urllib.parse import unquote

import yaml

HEADING = re.compile(r"#{1,6}\s+(.*?)\s*#*\s*$")
HTML_ANCHOR = re.compile(r"""<a\s+(?:id|name)=["']([^"']+)["']""")
INLINE_LINK = re.compile(r'\[[^\]]*\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\)')
REFERENCE = re.compile(r"^\s{0,3}\[[^\]]+\]:\s*<?(\S+?)>?(?:\s|$)")
SCHEME = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:|^//")


def prose(text, code=False):
    """Lines outside fenced code blocks; inline code dropped unless code."""
    fence = None
    for number, line in enumerate(text.splitlines(), 1):
        marker = re.match(r"\s{0,3}(`{3,}|~{3,})", line)
        if marker:
            if fence is None:
                fence = marker.group(1)
            elif marker.group(1).startswith(fence):
                fence = None
            continue
        if fence is None:
            yield number, line if code else re.sub(r"(`+).*?\1", "", line)


def anchors(text):
    """GitHub heading slugs (duplicates get -1, -2, ...) and HTML ids."""
    found, seen = set(), {}
    for _, line in prose(text, code=True):
        found.update(HTML_ANCHOR.findall(line))
        heading = HEADING.match(line)
        if not heading:
            continue
        title = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", heading.group(1))
        slug = re.sub(r"[^\w\- ]", "", title.lower()).replace(" ", "-")
        count = seen.get(slug, 0)
        seen[slug] = count + 1
        found.add(f"{slug}-{count}" if count else slug)
    return found


def broken_links(documents, exists):
    """Relative links in Markdown documents that miss a file or anchor.

    documents maps repository paths of .md files to their text; exists tells
    whether a repository path is a file or directory.
    """
    slugs = {name: anchors(text) for name, text in documents.items()}
    errors = []
    for name, text in documents.items():
        for number, line in prose(text):
            targets = INLINE_LINK.findall(line) + REFERENCE.findall(line)
            for target in targets:
                if SCHEME.match(target):
                    continue
                path, _, anchor = unquote(target).partition("#")
                resolved = name
                if path:
                    base = PurePosixPath(name).parent
                    resolved = posixpath.normpath(str(base / path))
                    if resolved.startswith("../") or not exists(resolved):
                        errors.append(f"{name}:{number}: missing {target}")
                        continue
                if anchor and resolved in slugs \
                        and anchor not in slugs[resolved]:
                    errors.append(f"{name}:{number}: missing anchor {target}")
    return errors


def main():
    output = subprocess.check_output(
        ["git", "-c", "safe.directory=/workspace", "ls-files", "--cached",
         "--others", "--exclude-standard", "-z", "--", ".",
         ":(exclude).beads/**", ":(exclude).beads.gate.lock"])
    names = sorted(set(output.decode().strip("\0").split("\0")))
    folded = {}
    documents = {}
    checked = 0
    for name in names:
        path = Path(name)
        if not path.is_file():
            continue
        key = name.casefold()
        if key in folded and folded[key] != name:
            raise ValueError(f"Case conflict: {name} / {folded[key]}")
        folded[key] = name
        data = path.read_bytes()
        if len(data) > 500 * 1024:
            raise ValueError(f"Repository file exceeds 500KiB: {name}")
        if b"\0" in data:
            continue
        try:
            text = data.decode("utf-8")
        except UnicodeDecodeError:
            continue
        if text and not text.endswith("\n"):
            raise ValueError(f"Missing final newline: {name}")
        for number, line in enumerate(text.splitlines(), 1):
            if line.startswith(("<<<<<<< ", ">>>>>>> ")):
                raise ValueError(f"Merge conflict: {name}:{number}")
            trailing = line[len(line.rstrip(" \t")):]
            if trailing and not (path.suffix == ".md" and trailing == "  "):
                raise ValueError(f"Trailing whitespace: {name}:{number}")
        if path.suffix == ".json":
            json.loads(text)
        elif path.suffix in (".yaml", ".yml"):
            list(yaml.safe_load_all(text))
        elif path.suffix == ".md":
            documents[name] = text
        checked += 1
    if not checked:
        raise ValueError("Empty repository validation inventory")
    errors = broken_links(documents, lambda name: Path(name).exists())
    if errors:
        raise ValueError("Broken Markdown links:\n" + "\n".join(errors))
    print(f"{checked} text files: whitespace, conflicts, JSON/YAML valid; "
          f"{len(documents)} Markdown files: relative links resolve")


if __name__ == "__main__":
    main()
