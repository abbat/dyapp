# Configuration diagnostics for Russian operators

Status: design; implementation is pending.

## Decision

Keep option names, file paths, TOML key paths, numeric locations and operating-system
error codes unchanged. Add a short Russian explanation of the failed stage when the
process locale is Russian. The English wording stays as it is except where hiding
configuration values is necessary to prevent secret disclosure.

Do not translate third-party error strings by matching their English text. For a
file-read failure, show the path, a Russian action for known kinds (missing file or
permission denied) or a generic read failure, and the original OS cause. For TOML
syntax failures, show the path, line and column when the parser provides them, and
a safe parser cause without echoing the source line. For type failures, show the
TOML key path and expected type when available, but omit the rejected value.
If a safe cause or field path cannot be extracted, use a generic Russian category
and retain the known location rather than print an untrusted value.

Example shapes (wording may change in implementation):

- `config /path/node.toml: ошибка синтаксиса TOML, строка 1, столбец 9: unclosed table`
- `config: неверный тип network.id_pow_bits, ожидается u32`
- `config /path/node.toml: файл не найден: No such file or directory (os error 2)`

The actual locale selection remains `LC_ALL`, then `LC_MESSAGES`, then `LANG`.
Machine-readable status fields, option names and persisted data do not change.

## Evidence and risk

`NodeConfig::load` currently interpolates the raw TOML and OS errors into
`config <path>: ...`; deserialization uses `config: ...`. In a real-binary check,
a malformed table showed the path, line, column, source excerpt and cause; a bad
type showed its rejected string and key path; a missing file showed the path and
OS error number. All three outputs were English under both `C` and
`ru_RU.UTF-8`.

The source excerpt is a security defect, not just untranslated text. A malformed
`turn.secret` line made its value appear in stderr. The TOML error API exposes a
message and source span separately, so formatting a location without printing the
source excerpt is viable; the implementation must still test that the message
itself contains no secret. Type errors need the same check because their current
text can contain the rejected value.

## Options

| Approach | Pros | Cons |
|---|---|---|
| Russian stage guidance with safe structured detail (chosen) | Actionable in Russian; retains paths, positions and causes; avoids brittle translations | Requires a formatter and explicit leak tests; some third-party cause words remain English |
| Prefix the existing error with Russian text | Very small change; preserves all existing details | Keeps full English text and the observed secret leak |
| Translate every parser and OS message | Could make more text Russian | Depends on unstable dependency and OS wording; can erase useful details or mishandle secrets |

## Verification and follow-up

First remove raw TOML source excerpts for both locales and add a real-binary
canary test. Then implement the Russian stage guidance. Test malformed TOML,
wrong types, missing files and denied reads in both locales. Assert that the
path and available line/column or key path survive, the canary never reaches
stderr, and the English diagnostics remain stable except for required secret
redaction. Review again if a dependency update changes parser error structure.
