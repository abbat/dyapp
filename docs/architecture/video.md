# Video Architecture (WebRTC)

## Overview

> ⚠️ **Prototype, not a video call.** `rust/video` can create a WebRTC offer
> and keep strings it is given. It cannot answer an offer, does not apply the
> remote answer or ICE candidates to the peer connection, sends no media and
> frame encryption is a stub. See
> [Encryption & Security Status](../security/encryption.md).

Source: `rust/video/src/session.rs` (session), `ice.rs` (candidate helpers),
`codec.rs` (codec preference helper), `encryption.rs` (frame encryption stub),
`rust/ffi/src/lib.rs` (FFI wrapper). This page was checked by reading the
source; the crate is not built in the test images, so nothing below was run.

**Transport split (target):** video uses the WebRTC stack (`webrtc` crate:
ICE for connectivity, DTLS for key exchange, SRTP/RTP for media). Messaging is
meant to use QUIC (`quinn`). They are separate transports; video does not run
over QUIC and `quinn` is not a WebRTC implementation.

## What exists today

| Piece | Status |
|-------|--------|
| `VideoSession::new()` | Creates an `RTCPeerConnection` with default settings: no media engine codecs registered, no tracks or transceivers, no ICE servers (no STUN/TURN) |
| `create_offer()` | Calls `create_offer` + `set_local_description`, returns the SDP. With no tracks the offer carries no media section |
| `mark_offer_sent()` | Manual state change; the caller says the offer was delivered |
| `receive_answer(sdp)` | Stores the string in `remote_sdp`; does **not** call `set_remote_description`, does not parse or validate the SDP |
| `add_ice_candidate(c)` | Appends to a local list; does **not** pass the candidate to the peer connection. Allowed in any state |
| `get_ice_candidates()` | Returns the candidates added above (remote ones), not locally gathered ones; no `on_ice_candidate` handler is registered |
| `mark_connected()` | Manual state change from **any** state; not driven by an ICE/DTLS connection event |
| `close()` | Closes the peer connection, state → `Closed` |
| Callee path (accept offer, create answer) | **Not implemented**: no method sets a remote offer or creates an answer |
| Media tracks, capture, rendering | Not implemented |
| Signaling channel | Not implemented; bootstrap has no SDP/ICE endpoint |
| Frame encryption | Stub (`encryption.rs`), see [below](#frame-encryption-target-design-not-implemented) |

## Session states

`SessionState` = `Idle`, `OfferCreated`, `OfferSent`, `AnswerReceived`,
`Connected`, `Failed`, `Closed`.

| From | Call | To | Error |
|------|------|----|-------|
| `Idle` | `create_offer()` | `OfferCreated` | any other state → `InvalidState("Offer already created")`; WebRTC failure → `OfferGenerationFailed` / `InvalidState` |
| `OfferCreated` | `mark_offer_sent()` | `OfferSent` | any other state → `InvalidState("Offer not created")` |
| `OfferSent` | `receive_answer(sdp)` | `AnswerReceived` | any other state → `InvalidState("Offer not sent yet")`; the SDP content is never checked |
| any | `mark_connected()` | `Connected` | never fails |
| any | `close()` | `Closed` | peer connection close error → `InvalidState` |

`Failed` is never set by the code. `AnswerGenerationFailed`, `ICEError` and
`EncryptionError` exist in `VideoError` but are never returned.

## Call setup

### Caller (what the API supports today)

```
Caller app                    VideoSession                Callee
  | create_offer()  ─────────▶ | set_local_description     |
  |◀──────── offer SDP ─────── | state = OfferCreated      |
  | [deliver offer: no signaling channel exists]  ─ ─ ─ ─ ▶|
  | mark_offer_sent() ───────▶ | state = OfferSent         |
  |◀ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ answer SDP ─ ─ |
  | receive_answer(answer) ──▶ | remote_sdp = answer       |
  |                            | state = AnswerReceived    |
  |                            | (answer NOT applied)      |
  | mark_connected() ────────▶ | state = Connected         |
  |                            | (no ICE/DTLS check)       |
```

`receive_answer` takes the callee's **answer** to the offer this session
created, never the caller's own offer.

### Callee (planned, no API)

```
Callee app                    VideoSession (planned)
  | receive offer via signaling
  | set_remote_offer(offer) ──▶ set_remote_description(offer)   (planned)
  | create_answer() ──────────▶ create_answer + set_local_description (planned)
  |◀──────── answer SDP
  | [deliver answer to caller]
```

### ICE (planned)

Target: each side registers `on_ice_candidate`, sends local candidates over
signaling (or waits for gathering to finish and sends the full SDP), and
passes remote candidates to `add_ice_candidate` on the peer connection. The
WebRTC stack then runs connectivity checks, DTLS, and reports a connection
state that should drive `Connected` / `Failed` instead of `mark_connected()`.

`ice.rs` only has helpers: `ICECandidate::priority()` returns 300/200/100/0 by
searching the candidate string for `host`/`srflx`/`relay`, and
`ICEGathering::best_candidate()` picks the maximum. These are not RFC 8445
priorities and are not used by the WebRTC stack.

No STUN or TURN server is configured, so only host candidates could be
gathered. TURN is not "always succeeds": a relay requires a deployed TURN
server with credentials, and the call fails if the relay is unreachable or
both peers are behind networks that block it. Bootstrap does not provide
TURN. Which TURN service to use is undecided.

## Codecs

`codec.rs` defines `VP8`, `VP9`, `H264`, `AV1` with payload types and a
`CodecNegotiation` helper. It is **not connected** to the peer connection: no
codecs are registered in the media engine, and the SDP is not affected.

`CodecNegotiation` semantics:

- `default_codecs()` = `[VP8, H264, VP9]` (local preference order).
- The agreed codec is the first **local** codec the remote list contains.
  With defaults, VP8 wins whenever the remote has it.
- If there is no common codec it falls back to the first local codec, i.e. a
  codec the remote does not support. Real negotiation must fail instead.

There is no platform-specific default (H.264 on mobile is a target idea, not
code), no resolution, frame rate or bitrate setting, and AV1 is only an enum
value.

## Frame Encryption (target design, not implemented)

`rust/video/src/encryption.rs` wraps the messaging stub: payload bytes are
returned unchanged, nonce and tag are random bytes, and `decrypt` does not
verify the tag. It is not called on any media path because none exists.

WebRTC media is already encrypted hop-by-hop with DTLS-SRTP once a connection
is established. The intended addition is end-to-end frame encryption on top
(insertable streams style), so that a TURN relay or a compromised SFU cannot
see content:

```
Raw frame
  ↓ encode (codec)
  ↓ encrypt payload with per-call key (ChaCha20-Poly1305)
  ↓ RTP packetize
  ↓ SRTP (DTLS-derived keys) over ICE-selected path
```

**Frame header metadata** (not encrypted by design): frame ID, timestamp,
width × height, nonce, auth tag (currently random and unchecked).

**Intended per-call key** (not implemented): derived from the MLS exporter of
the chat's group [ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md). Offer and answer,
with their DTLS fingerprints, travel as MLS messages, so MLS authenticates the
fingerprints [ADR 0006](../decisions/0006-transport-keys-separate-from-identity.md).

## FFI Boundary

`rust/ffi/src/lib.rs` exports `VideoSession` with `new`, `create_offer`,
`mark_offer_sent`, `receive_answer`, `close`, `get_session_id`. The underlying
`RustVideoSession` is created lazily on the first `create_offer`, so
`receive_answer` before it returns "Session not initialized". `add_ice_candidate`,
`mark_connected` and state queries are not exported. Errors cross as
`String`. No platform app loads the library yet.

## Security and metadata

- Media: no media flows today. Target hop-by-hop protection is DTLS-SRTP;
  end-to-end frame encryption is a stub.
- SDP and ICE candidates are not encrypted by WebRTC; they carry IP addresses
  and DTLS fingerprints. No signaling channel exists; whoever relays it will
  see peer IDs, candidate IPs and call timing. Signaling must be
  authenticated, or a relay can swap fingerprints and sit in the middle.
- A direct connection reveals each peer's IP to the other; a TURN server sees
  both IPs and traffic volume.
- `FrameHeader` fields reveal resolution, frame rate and duration to anyone on
  the media path.

See [Privacy & Metadata Visibility](../security/privacy.md).

## Testing

Unit tests (`rust/video/src/*.rs`):
- Session creation, `create_offer` → `mark_offer_sent` → `receive_answer("fake-sdp")`
  state changes, `mark_connected` from `Idle`, `close`
- `receive_answer` before an offer is rejected
- ICE helper string matching and `best_candidate`
- `CodecNegotiation` selection
- Frame encryption stub roundtrip (not a security test)

Integration tests (`tests/integration_tests.rs`, one process, no network):
`test_video_session_lifecycle` (one session, manual `mark_connected`),
`test_ice_candidate_priority`, `test_codec_negotiation`.

No test connects two sessions, exchanges a real answer or candidates, or
sends media.

## Planned work

- Callee API (set remote offer, create answer) and applying the answer
- Local candidate events and applying remote candidates
- Connection state from WebRTC events instead of `mark_connected`
- Media engine codecs, tracks, capture/render on each platform
- STUN/TURN configuration and a signaling channel
- End-to-end frame encryption with real AEAD and key exchange
- A two-peer test in Docker that reaches `connected` and exchanges frames
