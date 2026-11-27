# Shared translation catalogue

Native catalogue generation is being implemented. Current production Android XML,
desktop JSON and Apple xcstrings remain their existing sources until their migrations land.
The generator is not wired into standard quality/CI yet.

The implemented projection helper is `tools/scripts/translation_templates.py`.
It accepts canonical text with named arguments, explicit string/integer argument types,
a target platform and native tokens for every argument. It emits a format template,
not user values; it never evaluates expressions.

Canonical fields use `{name}`. Reordering or repeating fields in a translation preserves
the declared arguments. Every declared argument must occur in each translation.
Empty text, invalid Unicode/XML characters, missing/unknown fields, attribute/index access,
conversions and format expressions are refused. Double braces represent literal braces.

Android bindings use positional `%1$s` or `%1$d`; Apple bindings use `%1$@`,
`%1$d` or `%1$lld`. Positions are unique and contiguous, and each token must match
the declared argument type. Desktop bindings use named `{field}` tokens and can retain
a native field name different from the canonical one. Different arguments cannot share
a native field. Literal desktop braces cannot impersonate a declared native argument.

For printf targets with arguments, literal percent signs are doubled; inserted native
tokens are kept intact. XML and native resource escaping belong to the subsequent
resource renderer, not this template projection.

`tests/test_translation_templates.py` exercises the actual current native positional
tokens, Unicode, repeated/reordered arguments, literal braces/percent signs and malformed
bindings. Run it through the Docker Python test inventory; SDK compilation and native UI
acceptance remain separate checks.

The source loader is `tools/scripts/translation_sources.py`. It reads `i18n/catalogue.json`
(schema 1: `default`, `languages`, `messages`) and one JSON text dictionary per configured
language. `languages` maps language tags to Android qualifiers; the default uses `values`.
Each message declares `arguments`, a semantic `context` and `bindings` from native keys to tokens.
Aliases are explicit. Loading validates every language and placeholder before output:
duplicate keys, non-finite numbers, unsupported schema/fields, unsafe qualifiers, symlink/oversized
sources, missing/extra messages or language files and duplicate native bindings fail.
The loader writes nothing; production shared sources and native migration remain planned.

`translation_renderers.py` builds requested native outputs entirely in memory.
Android gets one `strings.xml` per configured qualifier, desktop gets one JSON file per language,
and Apple gets one xcstrings table containing all configured languages. Native keys and values
are sorted deterministically; source language and translated-unit metadata remain explicit.
Android strings use AAPT quotes/escapes before XML encoding so whitespace, quotes, backslashes,
newlines and literal reference prefixes survive. Byte/golden tests verify those boundaries;
they do not replace actual Android resource compilation or native UI acceptance.
Inside the Docker check container, `python3 -B tools/scripts/generate-translations.py` generates
the selected native resources. `--root REPO` selects a source/output project (including a fixture
under `build/`); repeat `--platform android|apple|desktop` to limit migration scope.
`--check` writes nothing and returns 1 for differing/missing generated output; invalid sources,
unsafe destinations and unmanaged native translation files return 2.
Every source, output byte set and destination is validated before generation writes changed files.
The tool never deletes unmanaged catalogues. Production source import and migrations remain planned.

The planned shared language sources must preserve existing native keys and wording,
mark platform-only contexts and keep semantic aliases explicit. Similar-looking text
alone does not establish that two messages have the same meaning.
