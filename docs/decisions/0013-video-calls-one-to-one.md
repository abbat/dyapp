# 0013. One-to-one video calls over libwebrtc, always end-to-end

- **Status:** Accepted
- **Date:** 2026-10-06

## Context

Calls must work between two peers behind NATs, relayed only by volunteer TURN nodes
([ADR 0007](0007-open-bootstrap-network.md)), and stay private from those relays. `rust/video` uses
the `webrtc` crate today.

## Decision

- **One-to-one calls only** for now. Group calls are a separate low-priority feature.
- **Always end-to-end encrypted** (DTLS-SRTP between the two peers, also through TURN); no setting.
  Call signalling is authenticated by MLS
  ([ADR 0006](0006-transport-keys-separate-from-identity.md)).
- **Codecs:** VP8 and H.264; VP9 where both sides support it.
- **Stack:** libwebrtc through FFI. No central SFU.

## Consequences

- `rust/video` must move from the `webrtc` crate to libwebrtc bindings; building libwebrtc for
  every platform is a significant build cost.
- Group calls need a later decision on topology and who hosts an SFU in an open network.
