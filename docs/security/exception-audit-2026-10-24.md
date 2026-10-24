# Repository exception audit — 2026-10-24

Owner: repository maintainer. Scope and extraction commands:
[exception audit inventory](../development/practices.md#exception-audit-inventory).
Next review: 2026-11-24, then monthly and immediately when an affected
dependency, scanner rule or code path changes. The next review has a deferred
tracking task; closing it requires scheduling its successor.

## Evidence and decisions

| Source checked | Evidence on 2026-10-24 | Decision and trigger |
|---|---|---|
| Rust and OSV advisory ignores | `deny.toml` has reasoned entries for `paste`, `basic-toml`, `proc-macro-error2` and the two `libcrux-kem` hybrid-KEM advisories. Root and fuzz OSV files mirror their applicable paths; Linux/Windows OSV files explain `glib` and `proc-macro-error`. `.cargo/audit.toml` mirrors the two `libcrux-kem` IDs. | Retain pending upstream fixes or dependency migration; re-check on every lockfile change and at the monthly review. The separate OpenMLS curve and zbus 5 findings remain open CI failures, not exceptions. |
| Dependency/tool ignores | `deny.toml` version skips carry reasons; Dependabot ignores `libc 0.2.190`, `argon2` updates and `aes-gcm >=0.11` with dependency constraints in comments. `check-dependency-age.py` exceptions carry dates and expire in 14 days. | Retain for now; revisit when iOS libc, snow/webrtc or OpenMLS dependency graphs move. The age check itself will reject stale entries after 2026-11-06. |
| GitHub alert dismissals | Paginated read-only API queries returned 36 dismissed code-scanning alerts and 2 dismissed Dependabot alerts. Every dismissal has a comment. The resolved secret-scanning query returned 0. Code-scanning reasons cover test vectors, public constants, local Docker images, the checked Gradle wrapper, and test-only diagnostics; Dependabot reasons cover `glib` on Linux/Windows. | Retain pending a changed trace, rule or affected dependency. Revisit all 38 at the monthly review; a new alert is evaluated separately before dismissal. |
| Secret-scan suppressions | `.gitleaksignore` has three fingerprint entries with comments (one historical TURN test vector, two `libcrux-secrets` crate names). One inline `gitleaks:allow` labels a fixed test vector. | Retain while the matching content remains non-secret; remove a fingerprint when no history scan needs it. |
| Escape hatches and mutation quarantine | Commit-trailer hatches are preserved in Git history and counted by the flow report. No production Rust `mutants::skip` or `mutants::exclude_re` annotation was found. | Review new hatches weekly in flow metrics and at release; history is not rewritten. No mutation quarantine action today. |
| Baselines and flaky quarantine | `baselines.toml` records measured coverage, panic, Scorecard, mutation, batch and source-size limits; `[flaky]` and `[size.files]` are empty. `artifact-sizes.json` records nine measured targets with revision and environment (seven also link CI evidence). Historical check cutoffs remain recorded in their scripts and docs. | Retain measured limits; tighten only after a comparable passing run. Never lower a gate for a red CI run. Review cutoffs if a script changes. |
| Local lint suppressions | Rust generated-code `allow` scopes, targeted Clippy reasons, Python `noqa: E402/F401`, Detekt test exclusions and the single cargo-shear generated-import ignore were inspected. | Retain generated/test-only cases. Add adjacent reasons to the Python import suppressions and recheck broad Detekt exclusions on the next tool update. |

## Follow-up

The first audit found no expired flaky quarantine or size-file exception.
Its follow-up added local explanations to the Python `noqa` imports and the
test-only lock guard's `dead_code` allowance. Individual GitHub dismissals
have no per-alert review date; the monthly schedule supplies a common review
date. This audit does not dismiss new alerts, change advisory ignores or reduce
any gate.
