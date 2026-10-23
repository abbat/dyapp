"""Nonmutating equivalents of basic repository hooks, run inside Docker."""
from html.parser import HTMLParser
import json
from pathlib import Path, PurePosixPath
import posixpath
import re
import subprocess
import tomllib
import unicodedata
from xml.etree import ElementTree
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
# Commit-message ratchet: commits after this published cutoff follow the rules.
# Moved on 2026-10-23 because 834083c was pushed with a lowercase subject;
# published history is not rewritten.
BASELINE = "834083cda0db08f32aec6682289ba729ecaead74"
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
DEBT_SOURCE = SOURCE | {".html", ".css", ".scss", ".java", ".gradle", ".c",
                        ".h", ".cpp", ".hpp", ".m", ".mm", ".ps1"}
# Split marker spellings so this source also passes its own check.
DEBT = re.compile(r"\b(?:T[O]DO|F[I]XME)\b")
SCOPE = re.compile(r"^[\w./-]+: ")
# Imperative words that look like past tense or a gerund.
IMPERATIVE = {"Bring", "Embed", "Exceed", "Feed", "Need", "Proceed", "Seed",
              "Shed", "Speed", "String", "Succeed"}
# Practice B3: protobuf and serde types each have a round-trip property.
PROPERTIES = "rust/bootstrap/tests/properties.rs"
PROTO_TYPE = re.compile(r"^\s*(?:message|enum)\s+(\w+)", re.M)
DERIVE = re.compile(r"#\[derive\(([^\]]*)\)\][^{;]*?\b(?:struct|enum)\s+(\w+)",
                    re.S)
PROPERTY = re.compile(r"^\s*\w+: (\w+)", re.M)
# Operator-written TOML, covered by the config tests instead.
ROUND_TRIP_EXEMPT = {"rust/bootstrap/src/config.rs"}
# Practice D1: visible UI strings live in the platform catalogues.
CATALOGUES = {"android": "clients/android/app/src/main/res/values/strings.xml",
              "apple": "clients/ios/DYApp/Localizable.xcstrings",
              "desktop": "clients/desktop/ui/i18n/en.json"}
KOTLIN_LITERAL = re.compile(r'\bText\(\s*"')
SWIFT_TEXT = re.compile(r'\bText\(\s*"((?:[^"\\]|\\.)*)"')
HTML_KEY = re.compile(r'data-i18n="([^"]+)"')
# Practice B4: every Service handler of a network request has a fuzz target.
HANDLERS = ("rust/bootstrap/src/service.rs",
            "rust/bootstrap/src/service/mailbox.rs",
            "rust/bootstrap/src/service/media.rs",
            "rust/bootstrap/src/service/search.rs")
HANDLER = re.compile(r"pub fn (\w+)\(\s*&self,[^)]*?"
                     r"(?:request: \w+Request|data: &\[u8\])")
SKIP = re.compile(r"^\s*#!?\[(?:cfg_attr\(.*)?mutants::(?:skip|exclude_re)\b")


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


def output_map_errors(entries, documents, exists):
    """Validate check formats and their repository documentation anchors."""
    if not isinstance(entries, dict) or not entries:
        return ["Check output map must be a nonempty object"]
    errors, references = [], []
    for name, entry in entries.items():
        if not re.fullmatch(r"[a-z][a-z0-9-]*", name):
            errors.append(f"Invalid check name: {name!r}")
        if not isinstance(entry, dict):
            errors.append(f"{name}: expected a check record")
            continue
        fix = entry.get("fix", "")
        path, separator, anchor = str(fix).partition("#")
        valid = all((path.startswith("docs/"), path.endswith(".md"),
                     ".." not in PurePosixPath(path).parts, separator, anchor))
        if not valid:
            errors.append(f"{name}: FIX needs a docs/*.md#anchor")
        else:
            references.append(f"[{name}]({fix})")
        kind = entry.get("format")
        if kind not in {"python", "json", "unittest", "rust-test", "lines"}:
            errors.append(f"{name}: unknown diagnostic format")
        if kind == "lines" or "pattern" in entry:
            try:
                re.compile(entry["pattern"])
            except (KeyError, TypeError, re.error):
                errors.append(f"{name}: invalid diagnostic pattern")
    synthetic = {**documents, "_check-output.md": "\n".join(references)}
    return errors + broken_links(synthetic, exists)


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


def debt_errors(texts):
    """Debt markers in source, including comments and string literals."""
    return [f"{name}:{number}: {match.group()} debt marker"
            for name, text in sorted(texts.items())
            if PurePosixPath(name).suffix in DEBT_SOURCE
            for number, line in enumerate(text.splitlines(), 1)
            for match in DEBT.finditer(line)]


def unicode_errors(names, texts):
    """Format (Cf: bidi, zero-width) and private-use (Co) characters, and
    non-ASCII paths: Trojan Source and lookalike file names."""
    errors = [f"{name}: non-ASCII path" for name in names
              if not name.isascii()]
    for name, text in sorted(texts.items()):
        if text.isascii():
            continue
        # UI text lives in string catalogues; SwiftLint and detekt cannot
        # check identifiers for lookalike letters on Linux.
        ascii_only = name.endswith((".swift", ".kt", ".kts"))
        for number, line in enumerate(text.splitlines(), 1):
            for column, char in enumerate(line):
                if ascii_only and not char.isascii():
                    errors.append(f"{name}:{number}: U+{ord(char):04X} "
                                  "in ASCII-only source")
                    continue
                category = unicodedata.category(char)
                if category not in ("Cf", "Co"):
                    continue
                # An emoji ZWJ sequence joins two pictographs.
                if char == "\u200d" and 0 < column < len(line) - 1 and \
                        unicodedata.category(line[column + 1]) == "So":
                    continue
                errors.append(f"{name}:{number}: U+{ord(char):04X} "
                              f"({category}) character")
    return errors


def manifest_errors(name, text):
    """Android: no backup or device transfer, and nothing exported but the
    launcher activity."""
    android = "{http://schemas.android.com/apk/res/android}"
    app = ElementTree.fromstring(text).find("application")
    if app is None:
        return []
    errors = []
    if app.get(android + "allowBackup") != "false":
        errors.append(f"{name}: allowBackup is not false")
    if not app.get(android + "dataExtractionRules"):
        errors.append(f"{name}: no dataExtractionRules")
    for component in app:
        launcher = "android.intent.category.LAUNCHER" in {
            category.get(android + "name")
            for category in component.iter("category")}
        if component.get(android + "exported") == "true" and not launcher:
            errors.append(f"{name}: {component.get(android + 'name')} "
                          "is exported")
    return errors


def round_trip_errors(texts):
    """Wire and storage types without a property in PROPERTIES."""
    tested = set(PROPERTY.findall(texts.get(PROPERTIES, "")))
    types = []
    for name, text in sorted(texts.items()):
        path = PurePosixPath(name)
        if path.match("proto/*.proto"):
            types += [(name, kind) for kind in PROTO_TYPE.findall(text)]
        elif path.suffix == ".rs" and path.parts[0] == "rust" \
                and "src" in path.parts and name not in ROUND_TRIP_EXEMPT:
            types += [(name, kind) for derive, kind in DERIVE.findall(text)
                      if {"Serialize", "Deserialize"} <= set(
                          re.findall(r"\w+", derive))]
    return [f"{name}: {kind} has no round-trip property"
            for name, kind in types if kind not in tested]


class _Visible(HTMLParser):
    """Text nodes of an HTML page outside <script> and <style>."""

    def __init__(self):
        super().__init__()
        self.skip, self.found = 0, []

    def handle_starttag(self, tag, attrs):
        self.skip += tag in ("script", "style")

    def handle_endtag(self, tag):
        self.skip -= tag in ("script", "style")

    def handle_data(self, data):
        if not self.skip and data.strip():
            self.found.append(data.strip())


def i18n_errors(texts):
    """UI string literals and keys missing from their catalogue."""
    errors = []
    apple = json.loads(texts.get(CATALOGUES["apple"], "{}")).get("strings", {})
    desktop = json.loads(texts.get(CATALOGUES["desktop"], "{}"))
    for name, text in sorted(texts.items()):
        path = PurePosixPath(name)
        if path.suffix == ".kt" and name.startswith(
                "clients/android/app/src/main/"):
            errors += [f"{name}: Text with a literal; use stringResource"
                       for _ in KOTLIN_LITERAL.finditer(text)]
        elif name.startswith(("clients/ios/DYApp/", "clients/macos/src/")) \
                and path.suffix == ".swift":
            errors += [f"{name}: Text key {key!r} not in "
                       f"{CATALOGUES['apple']}"
                       for key in SWIFT_TEXT.findall(text) if key not in apple]
        elif name.startswith("clients/desktop/ui/") and path.suffix == ".html":
            visible = _Visible()
            visible.feed(text)
            errors += [f"{name}: literal text {data!r}; use data-i18n"
                       for data in visible.found]
            errors += [f"{name}: data-i18n key {key!r} not in "
                       f"{CATALOGUES['desktop']}"
                       for key in HTML_KEY.findall(text) if key not in desktop]
    return errors


def fuzz_errors(texts):
    """Network request handlers that no fuzz target calls."""
    targets = "".join(text for name, text in texts.items()
                      if name.startswith("tests/fuzz/fuzz_targets/"))
    return [f"{name}: Service::{handler} has no fuzz target"
            for name in HANDLERS
            for handler in HANDLER.findall(texts.get(name, ""))
            if f"SERVICE.{handler}(" not in targets]


def skip_errors(texts):
    """Practice B2: a mutants attribute needs a // reason on it or above."""
    errors = []
    for name, text in sorted(texts.items()):
        if not name.endswith(".rs"):
            continue
        lines = [""] + text.splitlines()
        errors += [f"{name}:{number}: mutants attribute without a reason"
                   for number, line in enumerate(lines)
                   if SKIP.match(line) and not any(
                       "//" in reason for reason in (
                           line, lines[number - 1].lstrip()[:2]))]
    return errors


def main():
    output = subprocess.check_output(
        ["git", "-c", "safe.directory=/workspace", "ls-files", "--cached",
         "--others", "--exclude-standard", "-z", "--", ".",
         ":(exclude).beads/**", ":(exclude).beads.gate.lock",
         ":(exclude)tests/fuzz/corpus/**"])
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
        elif path.name == "AndroidManifest.xml":
            errors = manifest_errors(name, text)
            if errors:
                raise ValueError("\n".join(errors))
        elif path.name == "Cargo.toml" and "[package]" in text:
            # Practice C5: unsafe is forbidden outside the UNSAFE allow-list.
            policy = UNSAFE.get(name, FORBID)
            if not policy.search(text):
                raise ValueError(f"No unsafe lint policy: {name}")
        texts[name] = text
        checked += 1
    if not checked:
        raise ValueError("Empty repository validation inventory")
    errors = debt_errors(texts)
    if errors:
        raise ValueError("Source debt markers:\n" + "\n".join(errors))
    errors = unicode_errors(names, texts)
    if errors:
        raise ValueError("Hidden or non-ASCII Unicode:\n" + "\n".join(errors))
    allowed = NOT_TRACKER | {
        crate for name, text in texts.items()
        if name.endswith("Cargo.toml") for crate in CRATE.findall(text)}
    leaks = [f"{name}: {', '.join(found)}" for name, text in texts.items()
             if (found := bead_ids(text, allowed))]
    if leaks:
        raise ValueError("Tracker IDs in files:\n" + "\n".join(leaks))
    errors = round_trip_errors(texts)
    if errors:
        raise ValueError(f"Round trips ({PROPERTIES}):\n" + "\n".join(errors))
    errors = i18n_errors(texts)
    if errors:
        raise ValueError("UI strings (practice D1):\n" + "\n".join(errors))
    errors = fuzz_errors(texts)
    if errors:
        raise ValueError("Fuzz targets (tests/fuzz/):\n" + "\n".join(errors))
    errors = skip_errors(texts)
    if errors:
        raise ValueError("Mutation skips:\n" + "\n".join(errors))
    errors = commit_errors(allowed)
    if errors:
        raise ValueError("Commit messages:\n" + "\n".join(errors))
    errors = broken_links(documents, lambda name: Path(name).exists())
    if errors:
        raise ValueError("Broken Markdown links:\n" + "\n".join(errors))
    errors = output_map_errors(
        json.loads(texts["tools/scripts/check-output.json"]),
        documents, lambda name: Path(name).exists())
    if errors:
        raise ValueError("Check output map:\n" + "\n".join(errors))
    errors = orphans(documents, folded.values())
    if errors:
        raise ValueError(f"Not linked from {INDEX}:\n" + "\n".join(errors))
    lines = texts["AGENTS.md"].count("\n")
    if lines > AGENTS_LINES:
        raise ValueError(f"AGENTS.md has {lines} > {AGENTS_LINES} lines")
    size = tomllib.loads(texts["tests/scripts/baselines.toml"])["size"]
    errors = "\n".join(size_errors(texts, size))
    if errors:
        raise ValueError(f"Size budgets ([size] in baselines.toml):\n{errors}")
    print(f"{checked} text files: whitespace, conflicts, JSON/YAML valid, "
          "workflows set top-level permissions, no format or private-use "
          "characters, ASCII paths, crates follow the unsafe "
          "policy, no tracker IDs, protobuf and serde types have round-trip "
          "properties, network handlers have fuzz targets, UI strings "
          "come from catalogues, "
          "mutants skips give a reason, "
          f"source files <= {size['max_lines']} "
          "lines or their budget; "
          f"{len(documents)} Markdown files: relative links "
          f"resolve, docs/ reachable from {INDEX}, AGENTS.md <= "
          f"{AGENTS_LINES} lines; commit messages after the baseline follow "
          "the rules")


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("repository")
    main()
