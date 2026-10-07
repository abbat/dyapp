# Documentation

> **Status:** derived from `Makefile` and the repository tree on 2026-10-07. CI builds no
> documentation and nothing is published: the docs are developer-only and the repository is
> private. Generating or publishing docs (GitHub Pages, DocC, Dokka) is planned only once there are
> user-facing or external-developer docs; note that on a private repository a Pages site is public
> unless the plan is Enterprise Cloud.

## What exists

| Output | Source | State |
|--------|--------|-------|
| Markdown docs | `docs/` (index: [docs/README.md](../README.md)) | Written by hand; read on GitHub or in a clone. |
| Rust API | `make doc` → `cargo doc --workspace --locked --offline --no-deps` in the dev container | All 6 crates, public items. Output stays in the Docker volume `workspace-cache` (`/workspace/target/doc`), not on the host; nothing exports it. |
| Swift / Kotlin API | none | No DocC catalog, no Dokka plugin. |
| Link check | none | Markdown links are not checked by any script or CI job. |

## Local use

```bash
make prepare   # once, online: builds the dev image (and the other test images)
make doc       # checks that rustdoc builds for all 6 crates; output stays in the container volume
```

To browse the Rust API on the host, run `cargo doc --workspace --no-deps` with a local 1.99.0
toolchain and open `target/doc/`.

Check relative Markdown links by hand when moving files.

## Writing API docs

- **Rust:** `///` on public items, with `# Errors` / `# Panics` / `# Examples` sections where they
  apply. Doc examples are compiled and run by `cargo test` (doctests), so keep them building.
- **Swift / Kotlin:** `///` and KDoc respectively. The platform shells currently have almost no
  public API, so there is little to document until the Rust bindings exist
  ([ffi-bindings.md](ffi-bindings.md)).
