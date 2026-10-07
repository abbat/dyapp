# Protobuf code generation

Rust types are generated from `proto/` at build time. Nothing generated is committed, and no
`protoc` is needed: the schemas are parsed by [protox](https://crates.io/crates/protox), a pure
Rust compiler, so `cargo build` works the same on a developer machine, in Docker and in CI.

| Crate | `build.rs` compiles | Generated types |
|-------|---------------------|-----------------|
| `rust/identity` | `proto/identity.proto` | `SignedRecord` |
| `rust/profile` | `proto/profile.proto` | `Profile` |
| `rust/p2p-net` | `proto/node.proto` | `dyapp_p2p_net::proto::*` (requests, replies, `Status`, `Role`) |

Each `build.rs` calls `protox::compile` and `prost_build::Config::compile_fds`; the output lands in
`OUT_DIR` and is pulled in with `include!`. `node.proto` imports `identity.proto`; p2p-net maps
`.dyapp.identity` to `::dyapp_identity` with `extern_path`, so both crates share one
`SignedRecord` type. Methods on generated types live next to the `include!` as ordinary `impl`
blocks.

Notes on the generated code:

- `prost` strips the enum prefix: `STATUS_OK` becomes `Status::Ok`. Enum fields are `i32`; use
  `Status::try_from(value)` and treat an unknown value as unspecified.
- A `oneof` variant unknown to this build decodes as `None`; the node answers
  `STATUS_UNSUPPORTED`.
- Signatures cover the exact transmitted bytes, never a re-encoding: protobuf has no canonical
  serialization. Decode the payload of a `SignedRecord`, but store and forward the original bytes.

Swift and Kotlin get no protobuf code: the apps talk to the core through UniFFI types, so
protobuf stays inside the Rust crates. The `protobuf-compiler` package in `docker/Dockerfile.dev`
is not used by the build.

Schema contents and evolution rules: [Protobuf schema](../architecture/protobuf-schema.md).
