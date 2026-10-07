# Documentation Build and Publishing

> **Status:** derived from `.github/workflows/docs.yml`, `Makefile` and the repository tree on
> 2026-10-06. Nothing here was run for this document; whether Pages is enabled and what is
> currently deployed is **unverified** (it lives in GitHub settings, not the repo).

## What exists

| Output | Source | State |
|--------|--------|-------|
| Markdown docs | `docs/` (index: [docs/README.md](../README.md)) | Written by hand; read on GitHub or in a clone. This is the only complete documentation. |
| Rust API (local) | `make doc` → `cargo doc --workspace --locked --offline --no-deps` in the dev container | All 6 crates, public items. Output stays in the Docker volume `workspace-cache` (`/workspace/target/doc`), not on the host; nothing exports it. |
| Rust API (CI) | `docs.yml` step "Build Rust docs" | 4 crates only, with `--document-private-items`; **output is lost**, see below. |
| Swift API | `docs.yml` | Placeholder page ("Build on macOS runner"); no DocC catalog exists. |
| Kotlin API | `docs.yml` | Not generated: no Dokka plugin in `android/`; the failure is suppressed. |
| Link check | none in `scripts/` | No `scripts/check-links.sh` or `scripts/build-docs.sh` exists. |

## The `docs.yml` workflow as written

**Triggers:** push to `main`, `master`, `develop` touching `src/**`, `ios/**`, `android/**`,
`docs/**`, `Cargo.toml` or the workflow itself; `workflow_dispatch`. Not triggered by `rust/**`
(where all crates live). There is no `src/` directory, so Rust API changes alone do not rebuild the
docs.

**Job `build-docs`** (ubuntu-latest, floating `stable` Rust, Node 18 set up but unused):

| Step | What it does | Result |
|------|--------------|--------|
| Build Rust docs | `cargo doc -p identity -p profile -p p2p-net -p messaging -p video --no-deps --document-private-items` (no `bootstrap`, no `ffi`), then `cp -r target/doc/* target/docs/rust/` | `target/docs/rust/` is never created, so `cp` fails; `2>/dev/null \|\| echo "Rust docs built"` hides it. **No Rust API is published.** |
| Swift docs | Writes a one-line HTML placeholder to `swift/index.html` | Placeholder |
| Kotlin docs | `./gradlew dokkaHtml 2>/dev/null \|\| echo "Dokka not configured yet"`, then copy | `kotlin/` is an empty directory |
| Copy markdown docs | `cp -r docs/* target/docs/guides/` | Raw `.md` files, **not rendered**; no `guides/index.html`, so `guides/` has no landing page on Pages |
| (same step) index | Heredoc writes `target/docs/index.html` | Landing page; see below |
| Upload | artifact `documentation` (`target/docs/`, 30 days) | Succeeds as long as `index.html` exists |

**Job `deploy-pages`:** on `master`/`main` only; uploads the artifact to GitHub Pages and deploys.
It needs Pages set to "GitHub Actions" in repository settings (unverified).

**Job `summary`:** prints "Documentation generated successfully" whenever `build-docs` succeeded,
which it does even when every API step produced nothing.

### Landing page links

The landing page is generated inline in `docs.yml`, so changing it means editing the workflow,
not `target/docs/index.html` (a build output).

| Link | Target in the artifact |
|------|------------------------|
| `rust/dyapp_*/index.html` (4) | Missing (copy failed) |
| `swift/` | Placeholder page |
| `kotlin/` | Empty directory |
| `guides/` | Directory of raw Markdown, no index |
| `guides/decisions/` | Raw Markdown of [docs/decisions/](../decisions/README.md), no index.html |
| `guides/contributing/` | Missing: `docs/contributing/` does not exist |

The "P2P Network" and "Security" cards list topics without links. In short: a successful
`docs.yml` run publishes a landing page whose API links are broken and whose guides are raw files.

## Local use

```bash
make prepare   # once, online: builds the dev image (and the other test images)
make doc       # checks that rustdoc builds for all 6 crates; output stays in the container volume
```

To browse the Rust API on the host, run `cargo doc --workspace --no-deps` with a local 1.99.0
toolchain and open `target/doc/`. There is no supported local equivalent of the Pages site; to
inspect a CI build, download the `documentation` artifact from the workflow run.

Markdown links are not checked by any repository script or CI job; check relative links by hand
when moving files.

## Fixing the pipeline (not done)

Code and workflow changes are outside this document. To make the published site match the landing
page, at minimum:

1. Trigger on `rust/**` instead of `src/**`.
2. `mkdir -p target/docs/rust` before the copy, drop the error suppression, and build all 6 crates
   with the pinned 1.99.0 toolchain (public items only, unless private docs are intended).
3. Either render Markdown (a static site generator with a nav) or publish nothing but links to the
   GitHub tree; do not link to directories without an index.
4. Remove the Swift/Kotlin cards until DocC/Dokka are configured, or make the steps fail when
   output is missing.
5. Add a link check (e.g. a script under `scripts/`) to `quality-in-container.sh`.

**Versioning:** a `docs-v1.0` branch would not trigger `docs.yml` (branch filter) and would not
deploy (deploy is `master`/`main` only); a Pages site has a single deployment, so versioned docs
would need explicit path-based output (e.g. `v1.0/`) in the workflow.

## Writing API docs

- **Rust:** `///` on public items, with `# Errors` / `# Panics` / `# Examples` sections where they
  apply. Doc examples are compiled and run by `cargo test` (doctests), so keep them building.
- **Swift / Kotlin:** `///` and KDoc respectively. The platform shells currently have almost no
  public API, so there is little to document until the Rust bindings exist
  ([ffi-bindings.md](ffi-bindings.md)).
