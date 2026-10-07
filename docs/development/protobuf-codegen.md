# Protobuf code generation

> **Status:** no code is generated from `.proto` files today, and nothing consumes generated code.
> The wire format in use is JSON over the bootstrap REST API. Which schema becomes canonical is
> open; see [Protobuf schema](../architecture/protobuf-schema.md).

## What exists

| Item | State |
|------|-------|
| `proto/core_messages.proto` (`dyapp.core`) | Draft schema. Referenced by nothing. |
| `proto/messages.proto` (`dyapp`) | Draft schema. Only referenced by `scripts/regenerate-proto.sh`. |
| `prost` in `rust/identity`, `rust/profile` | Used through `#[derive(prost::Message)]` structs written by hand; no `build.rs`, nothing generated from `proto/`. |
| `prost` in `rust/messaging` | Declared, unused. `prost-build` is only in `[workspace.dependencies]`. |
| `protoc` | Installed in the dev image (`protobuf-compiler` in `docker/Dockerfile.dev`); not used by any target. |
| `src/generated/`, `ios/Generated/`, `android/.../generated/`, `.proto-checksums` | Do not exist. |
| CI proto validation | None. No workflow regenerates or diffs protobuf output. |
| Swift / Kotlin protobuf | No `swift-protobuf` or `protobuf-kotlin` dependency in `ios/` or `android/`. |

## `scripts/regenerate-proto.sh` as written

It is not called by the Makefile, CI, or any build. If run, it:

1. Requires `protoc` on `PATH`; then runs `cargo build -p dyapp-profile` only to grep for
   "prost" (a warning, never a failure).
2. **Rust:** writes a `build_proto.rs` file, runs `protoc --prost_out=src/generated`, prints
   "Using prost-build instead" if that fails, then deletes `build_proto.rs` without running it.
   `--prost_out` needs the `protoc-gen-prost` plugin, which is not installed or checked, so the
   fallback message is the likely result and no Rust code is produced.
3. **Swift:** `protoc --swift_out=ios/Generated` if `protoc-gen-swift` is present, else skips.
4. **Kotlin:** `--kotlin_out`, or `--java_out` if `protoc-gen-kotlin` is missing, into
   `android/app/src/main/kotlin/generated`.
5. **Go:** into `bootstrap/pkg/pb` if `protoc-gen-go` exists. There is no Go bootstrap; the
   bootstrap is the Rust crate `rust/bootstrap`.
6. Fails verification when `src/generated/dyapp.rs` is missing (so step 2's failure ends
   the run here, with exit 1).
7. Would write `.proto-checksums` with SHA-256 of sources and outputs. A checksum recorded after
   generation only proves the files did not change since; it does not prove they match the
   `.proto` sources.

It generates from `messages.proto`; nothing uses `core_messages.proto`.

## Proposed strategy (not adopted)

To be decided together with the canonical schema; record the choice as an
[ADR](../decisions/README.md).

- **Rust:** build-time generation with `prost-build` in the `build.rs` of the crate that owns the
  wire types, `OUT_DIR` output, `include!` into a module, `use prost::Message` for
  `encode`/`decode`. Needs `protoc` at build time (present in the dev image; set `PROTOC` or
  vendor it for builds outside Docker). Nothing is committed.
- **Swift / Kotlin:** not planned. The apps talk to the core through UniFFI types, so protobuf
  stays inside the Rust crates; the Swift and Kotlin steps of
  `regenerate-proto.sh` are to be removed.
- **Signatures** cover the exact transmitted bytes, never a re-encoding: protobuf has no canonical
  serialization.
- Timestamps cross languages as `int64` with a stated unit (milliseconds since the Unix epoch, as
  `Message.created_at` in `rust/messaging`), not as floating-point seconds.

Until then: do not run `regenerate-proto.sh` expecting usable output, and do not add generated
files by hand.
