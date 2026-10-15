"""Nonmutating equivalents of basic repository hooks, run inside Docker."""
import json
from pathlib import Path, PurePosixPath
import posixpath
import re
import subprocess
import tomllib
from urllib.parse import unquote

import yaml

HEADING = re.compile(r"#{1,6}\s+(.*?)\s*#*\s*$")
HTML_ANCHOR = re.compile(r"""<a\s+(?:id|name)=["']([^"']+)["']""")
INLINE_LINK = re.compile(r'\[[^\]]*\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\)')
REFERENCE = re.compile(r"^\s{0,3}\[[^\]]+\]:\s*<?(\S+?)>?(?:\s|$)")
SCHEME = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:|^//")
# Tracker IDs (a 3-4 character dyapp- suffix, optionally .N) stay out of the
# repository; longer hyphenated names (dyapp-p2p-net) do not match.
BEAD = re.compile(r"\bdyapp-[a-z0-9]{3,4}(?:\.\d+)*(?![\w-])")
CRATE = re.compile(r'^name\s*=\s*"(dyapp-[\w-]+)"', re.M)
# Names that are not crates: the stale package name in build-uniffi.sh.
NOT_TRACKER = {"dyapp-core"}
# Commit-message ratchet: commits after this one follow the message rules.
BASELINE = "39c3e3fd8642497fca3003efa97bafd75008268f"
# Practice A9: every docs/ file is reachable from the index; AGENTS.md short.
INDEX = "docs/README.md"
AGENTS_LINES = 80
FORBID = re.compile(r'^\[lints\]\nworkspace = true$'
                    r'|^unsafe_code = "forbid"$', re.M)
DOCUMENTED = re.compile(r'^undocumented_unsafe_blocks = "deny"$', re.M)
UNSAFE = {"rust/bootstrap/Cargo.toml": DOCUMENTED,
          "rust/ffi/Cargo.toml": DOCUMENTED}
# Practice B6: source files whose line count is budgeted.
SOURCE = {".rs", ".py", ".sh", ".kt", ".kts", ".swift", ".ts", ".tsx", ".js",
          ".mjs", ".proto"}
SCOPE = re.compile(r"^[\w./-]+: ")
# Imperative words that look like past tense or a gerund.
IMPERATIVE = {"Bring", "Embed", "Exceed", "Feed", "Need", "Proceed", "Seed",
              "Shed", "Speed", "String", "Succeed"}


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
        for number, target, resolved, anchor in links(name, text):
            if resolved.startswith("../") or not exists(resolved):
                errors.append(f"{name}:{number}: missing {target}")
            elif anchor and resolved in slugs \
                    and anchor not in slugs[resolved]:
                errors.append(f"{name}:{number}: missing anchor {target}")
    return errors


def links(name, text):
    """(line, target, repository path, anchor) of relative links."""
    for number, line in prose(text):
        for target in INLINE_LINK.findall(line) + REFERENCE.findall(line):
            if SCHEME.match(target):
                continue
            path, _, anchor = unquote(target).partition("#")
            resolved = name
            if path:
                base = PurePosixPath(name).parent
                resolved = posixpath.normpath(str(base / path))
            yield number, target, resolved, anchor


def orphans(documents, files, index=INDEX):
    """Files under docs/ that no link chain from the index reaches."""
    seen, queue = {index}, [index]
    while queue:
        name = queue.pop()
        for _, _, resolved, _ in links(name, documents.get(name, "")):
            if resolved not in seen:
                seen.add(resolved)
                queue.append(resolved)
    return [name for name in files
            if name.startswith("docs/") and name not in seen]


def bead_ids(text, allowed):
    """Tracker IDs in text that are not allowed crate names."""
    return [m for m in BEAD.findall(text) if m not in allowed]


def message_errors(message, allowed, bot=False):
    """Chris Beams' rules that a script can check, plus agent attribution.

    Bots (Dependabot) need no Co-Authored-By trailer.
    """
    subject, _, body = message.strip().partition("\n")
    errors = []
    if len(subject) > 72:
        errors.append(f"subject longer than 72 characters ({len(subject)})")
    if subject.endswith("."):
        errors.append("subject ends with a period")
    if body and not body.startswith("\n"):
        errors.append("no blank line after the subject")
    word = SCOPE.sub("", subject).split(" ", 1)[0]
    if not word[:1].isupper():
        errors.append("subject does not start with a capital letter")
    elif word not in IMPERATIVE and re.search(r"(ed|ing|[^sui]s)$", word):
        errors.append(f"subject is not imperative: {word}")
    if not bot and not re.search(r"^Co-Authored-By: .+ <.+>$", message, re.M):
        errors.append("no Co-Authored-By trailer")
    leaks = bead_ids(message, allowed)
    if leaks:
        errors.append("tracker IDs: " + ", ".join(leaks))
    return errors


def commit_errors(allowed):
    """Message rule violations in commits after BASELINE."""
    git = ["git", "-c", "safe.directory=/workspace"]
    # A shallow clone without the baseline cannot run the check: fail.
    subprocess.check_call(git + ["cat-file", "-e", BASELINE + "^{commit}"])
    output = subprocess.check_output(
        git + ["log", "--no-merges", "--format=%H %ae%n%B%x00",
               BASELINE + "..HEAD"]).decode()
    errors = []
    for entry in filter(str.strip, output.split("\0")):
        head, _, message = entry.strip().partition("\n")
        sha, _, author = head.partition(" ")
        bot = author.endswith("[bot]@users.noreply.github.com")
        errors += [f"{sha[:12]}: {error}"
                   for error in message_errors(message, allowed, bot)]
    return errors


def size_errors(texts, size):
    """Source files over size["max_lines"] must match size["files"]."""
    limit, budgets = size["max_lines"], size.get("files", {})
    errors = []
    for name, text in sorted(texts.items()):
        if PurePosixPath(name).suffix not in SOURCE:
            continue
        lines = text.count("\n")
        budget = budgets.get(name, limit)
        if lines > budget:
            errors.append(f"{name}: {lines} > {budget} lines")
        elif name in budgets and lines < budget:
            drop = " (or drop it)" if lines <= limit else ""
            errors.append(f"{name}: {lines} lines, lower its budget{drop}")
    errors += [f"{name}: budgeted but not tracked"
               for name in sorted(budgets.keys() - texts.keys())]
    return errors


def main():
    output = subprocess.check_output(
        ["git", "-c", "safe.directory=/workspace", "ls-files", "--cached",
         "--others", "--exclude-standard", "-z", "--", ".",
         ":(exclude).beads/**", ":(exclude).beads.gate.lock"])
    names = sorted(set(output.decode().strip("\0").split("\0")))
    folded = {}
    documents = {}
    texts = {}
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
            loaded = list(yaml.safe_load_all(text))
            # Practice C4: a default-permission token is never implicit.
            workflow = name.startswith(".github/workflows/")
            if workflow and "permissions" not in loaded[0]:
                raise ValueError(f"No top-level permissions: {name}")
        elif path.suffix == ".md":
            documents[name] = text
        elif path.name == "Cargo.toml" and "[package]" in text:
            # Practice C5: unsafe is forbidden outside the UNSAFE allow-list.
            policy = UNSAFE.get(name, FORBID)
            if not policy.search(text):
                raise ValueError(f"No unsafe lint policy: {name}")
        texts[name] = text
        checked += 1
    if not checked:
        raise ValueError("Empty repository validation inventory")
    allowed = NOT_TRACKER | {
        crate for name, text in texts.items()
        if name.endswith("Cargo.toml") for crate in CRATE.findall(text)}
    leaks = [f"{name}: {', '.join(found)}" for name, text in texts.items()
             if (found := bead_ids(text, allowed))]
    if leaks:
        raise ValueError("Tracker IDs in files:\n" + "\n".join(leaks))
    errors = commit_errors(allowed)
    if errors:
        raise ValueError("Commit messages:\n" + "\n".join(errors))
    errors = broken_links(documents, lambda name: Path(name).exists())
    if errors:
        raise ValueError("Broken Markdown links:\n" + "\n".join(errors))
    errors = orphans(documents, folded.values())
    if errors:
        raise ValueError(f"Not linked from {INDEX}:\n" + "\n".join(errors))
    lines = texts["AGENTS.md"].count("\n")
    if lines > AGENTS_LINES:
        raise ValueError(f"AGENTS.md has {lines} > {AGENTS_LINES} lines")
    size = tomllib.loads(texts["scripts/baselines.toml"])["size"]
    errors = "\n".join(size_errors(texts, size))
    if errors:
        raise ValueError(f"Size budgets ([size] in baselines.toml):\n{errors}")
    print(f"{checked} text files: whitespace, conflicts, JSON/YAML valid, "
          "workflows set top-level permissions, crates follow the unsafe "
          f"policy, no tracker IDs, source files <= {size['max_lines']} "
          "lines or their budget; "
          f"{len(documents)} Markdown files: relative links "
          f"resolve, docs/ reachable from {INDEX}, AGENTS.md <= "
          f"{AGENTS_LINES} lines; commit messages after the baseline follow "
          "the rules")


if __name__ == "__main__":
    main()
