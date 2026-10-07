# 0014. Clients and nodes talk over libp2p only

- **Status:** Accepted
- **Date:** 2026-10-07

## Context

The bootstrap node serves axum REST routes over plain HTTP, while the network itself is a
libp2p DHT ([ADR 0008](0008-sybil-and-eclipse-defences.md)). Two stacks mean two transports,
two authentication schemes and two sets of tests. The project has no gRPC: `proto/` holds unused
schemas and `identity` and `profile` define their `prost` types by hand. gRPC needs HTTP/2 and
does not run over libp2p streams.

## Decision

- **libp2p only** between clients (desktop and mobile) and nodes, and between nodes: no REST, no
  gRPC.
- **Requests and replies are protobuf messages** over libp2p request-response, one protocol ID per
  service; the schemas live in `proto/` and Rust types are generated from them.
- **Authorisation:** the Noise peer ID is the transport key
  ([ADR 0006](0006-transport-keys-separate-from-identity.md)); a request that acts on an
  identity's data carries a record signed by that identity.
- **Call media is the exception:** it goes over WebRTC with TURN run by network nodes
  ([ADR 0013](0013-video-calls-one-to-one.md)); libp2p only finds those nodes.

## Consequences

- One transport, one authentication scheme, one test harness for the node; it can be tested in
  Docker on Linux without any app.
- Mobile apps run rust-libp2p in the core; reaching a sleeping phone (push wake-up) is not solved
  by libp2p and is deferred.
- libp2p has no traffic obfuscation; resistance to DPI blocking is deferred.
- Not implemented: `rust/bootstrap` still serves REST (`api.rs`); `rust/p2p-net` has the libp2p
  node but no application protocols.
