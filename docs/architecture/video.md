# Video Architecture (WebRTC)

## Overview

> ⚠️ **Prototype, not a video call.** `rust/video` can create a WebRTC offer
> and apply the remote answer and ICE candidates to the peer connection. It
> cannot answer an offer, does not report its own candidates and sends no media;
> SFrame frame encryption exists but nothing feeds it frames. See
> [Encryption & Security Status](../security/encryption.md).

Source: `rust/video/src/session.rs` (session), `ice.rs` (candidate helpers),
`codec.rs` (codec preference helper), `sframe.rs` (frame encryption),
`rust/ffi/src/lib.rs` (FFI wrapper).

**Transport split (target):** video uses the WebRTC stack (`webrtc` crate 0.21,
an async layer over the sans-I/O `rtc` core: ICE for connectivity, DTLS for key
exchange, SRTP/RTP for media). Messaging is
meant to use libp2p. They are separate transports; video does not run over
libp2p.

The authored session, codec and ICE APIs have source documentation enforced by
the staged public documentation budget. Codec matching compares formats only;
hardware and open-source flags are static classifications, not capability probes.

## What exists today

| Piece | Status |
|-------|--------|
| `VideoSession::new()` | Builds a `PeerConnection` with webrtc's default codecs, UDP sockets on every local interface (`0.0.0.0:0`), one send/receive video transceiver with no track, no ICE servers (no STUN/TURN) and mDNS off (remote `.local` candidates, as browsers send, are not resolved). The event handler ignores all events |
| `create_offer()` | Calls `create_offer` + `set_local_description`, returns the SDP with one video media section |
| `mark_offer_sent()` | Manual state change; the caller says the offer was delivered |
| `receive_answer(sdp)` | Parses the SDP as an answer and calls `set_remote_description`; on success stores it in `remote_sdp` and applies the candidates added so far |
| `add_ice_candidate(c)` | Stores a remote candidate; once the answer is set, also passes it to the peer connection (`ICEError` if rejected). Allowed in any state |
| `get_ice_candidates()` | Returns the candidates added above (remote ones), not locally gathered ones; local `on_ice_candidate` events are ignored |
| `mark_connected()` | Manual state change from **any** state; not driven by an ICE/DTLS connection event |
| `close()` | Closes the peer connection, state → `Closed` |
| Callee path (accept offer, create answer) | **Not implemented**: no method sets a remote offer or creates an answer |
| Media tracks, capture, rendering | Not implemented |
| Signaling channel | Not implemented; bootstrap has no SDP/ICE endpoint |
| Frame encryption | `sframe::Sender` and `sframe::Receiver`, not on a media path; see [below](#frame-encryption) |

## Session states

`SessionState` = `Idle`, `OfferCreated`, `OfferSent`, `AnswerReceived`,
`Connected`, `Failed`, `Closed`.

| From | Call | To | Error |
|------|------|----|-------|
| `Idle` | `create_offer()` | `OfferCreated` | any other state → `InvalidState("Offer already created")`; WebRTC failure → `OfferGenerationFailed` / `InvalidState` |
| `OfferCreated` | `mark_offer_sent()` | `OfferSent` | any other state → `InvalidState("Offer not created")` |
| `OfferSent` | `receive_answer(sdp)` | `AnswerReceived` | any other state → `InvalidState("Offer not sent yet")`; SDP that does not parse or is rejected by the peer connection → `InvalidState`, state unchanged |
| any | `mark_connected()` | `Connected` | never fails |
| any | `close()` | `Closed` | peer connection close error → `InvalidState` |

`Failed` is never set by the code. `AnswerGenerationFailed` and
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
  | receive_answer(answer) ──▶ | set_remote_description    |
  |                            | state = AnswerReceived    |
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

### ICE

Remote candidates reach the peer connection: `add_ice_candidate` applies them
once the answer is set, and `receive_answer` applies those that came earlier.

Planned: each side handles `on_ice_candidate` and sends local candidates over
signaling (or waits for gathering to finish and sends the full SDP). The
WebRTC stack then runs connectivity checks, DTLS, and reports a connection
state that should drive `Connected` / `Failed` instead of `mark_connected()`.

`ice.rs` only has helpers: `ICECandidate::priority()` returns 300/200/100/0 by
searching the candidate string for `host`/`srflx`/`relay`, and
`ICEGathering::best_candidate()` picks the maximum. These are not RFC 8445
priorities and are not used by the WebRTC stack.

No STUN or TURN server is configured, so only host candidates could be
gathered. TURN is not "always succeeds": a relay requires a deployed TURN
server with credentials, and the call fails if the relay is unreachable or
both peers are behind networks that block it. A `dyappd` node with the `turn`
role hands out short-lived credentials for a coturn relay its operator runs
([TURN relay](../manuals/bootstrap.md#turn-relay)); the video code does
not request them yet.

## Codecs

`codec.rs` defines `VP8`, `VP9`, `H264`, `AV1` with payload types and a
`CodecNegotiation` helper. It is **not connected** to the peer connection: the
media engine registers webrtc's own default codecs, and `codec.rs` does not
affect the SDP.

`CodecNegotiation` semantics:

- `default_codecs()` = `[VP8, H264, VP9]` (local preference order).
- The agreed codec is the first **local** codec the remote list contains.
  With defaults, VP8 wins whenever the remote has it.
- If there is no common codec nothing is agreed: `has_agreed()` is false and
  `agreed_codec()` is `None`.

There is no platform-specific default (H.264 on mobile is a target idea, not
code), no resolution, frame rate or bitrate setting, and AV1 is only an enum
value.

## Frame Encryption

`rust/video/src/sframe.rs` implements SFrame, but no media path exists to use it; the
algorithm and its checks are in [Encryption & Security Status](../security/encryption.md).

WebRTC media is already encrypted hop-by-hop with DTLS-SRTP once a connection
is established. The intended addition is end-to-end frame encryption on top
(insertable streams style), so that a TURN relay or a compromised SFU cannot
see content:

```
Raw frame
  ↓ encode (codec)
  ↓ encrypt payload with SFrame (RFC 9605), keys from the MLS exporter
  ↓ RTP packetize
  ↓ SRTP (DTLS-derived keys) over ICE-selected path
```

**Frame metadata** (not encrypted): the SFrame header carries the key ID and
counter; frame sizes and timing stay visible.

**Per-call key**: the base key is `Group::export_secret(device, sframe::EXPORTER_LABEL, b"",
sframe::BASE_KEY_BYTES)` of the chat's group in each epoch; no call path calls it yet [ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md). Offer and answer,
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
  end-to-end SFrame encryption exists in `rust/video` but is not wired to media.
- SDP and ICE candidates are not encrypted by WebRTC; they carry IP addresses
  and DTLS fingerprints. No signaling channel exists; whoever relays it will
  see peer IDs, candidate IPs and call timing. Signaling must be
  authenticated, or a relay can swap fingerprints and sit in the middle.
- A direct connection reveals each peer's IP to the other; a TURN server sees
  both IPs and traffic volume.
- Frame sizes and timing reveal resolution, frame rate and duration to anyone
  on the media path.

See [Privacy & Metadata Visibility](../security/privacy.md).

## Testing

Unit tests (`rust/video/src/*.rs`):
- Session creation, `create_offer` → `mark_offer_sent` state changes,
  `mark_connected` from `Idle`, `close`
- `receive_answer` with a real answer from a second peer connection, with a
  candidate added before it; invalid SDP and an answer before an offer are
  rejected
- ICE helper string matching and `best_candidate`
- `CodecNegotiation` selection
- SFrame: RFC 9605 header and encryption test vectors, tampering, wrong key,
  replay window, counter uniqueness and epoch retirement

Integration tests (`tests/integration_tests.rs`, one process, no network):
`test_video_session_lifecycle` (one session, manual `mark_connected`),
`test_ice_candidate_priority`, `test_codec_negotiation`.

No test reaches a connected state or sends media.

## Planned work

- Callee API (set remote offer, create answer)
- Local candidate events
- Connection state from WebRTC events instead of `mark_connected`
- Media engine codecs, tracks, capture/render on each platform
- STUN/TURN configuration and a signaling channel
- SFrame on the media path, its base key exported per epoch from the chat's MLS group
- A two-peer test in Docker that reaches `connected` and exchanges frames
