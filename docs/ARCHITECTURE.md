# Implemented architecture

The repository contains working components, without placeholder media modules.

```text
apps/desktop             Tauri 2, React, TypeScript, Tailwind
apps/cli                 native diagnostics, room controls, transport benchmark
crates/core              standard input, curves, remapping, owner/slot guards, watchdog, percentiles
crates/gamepad           exact SDL3 C ABI bindings, explicit library path, hotplug/state/rumble
crates/input-transport   56-byte v1 codec, session/owner/slot/sequence receiver
crates/networking        real WebRTC, dual channels, HMAC signaling, selected pair, benchmark
crates/virtual-gamepad   replaceable backend trait, VIIPER TCP feeder, independent XInput reader
crates/session           native SDL worker and authorized async host/guest lifecycle
services/signaling      bounded Axum WebSocket rooms and approval routing
```

The UI polls immutable native snapshots at ~30 Hz; input sampling is independent, with a 4 ms scheduling interval. The guest transmits full states at at most 125 Hz. Neither number is advertised as the device's native report rate. The state channel has zero retransmits and unordered delivery. Congestion skips old snapshots. Button changes also send a complete state through the reliable channel; the shared monotonic sequence prevents delayed edges from overriding newer full states.

The 250 ms watchdog advances only for valid, authorized, increasing-sequence reports. Replayed or malformed frames do not extend it. Disconnect/revoke/actor error attempts neutral output and destroys only this session's dedicated VIIPER bus. After a process crash, the independently configured VIIPER stream-disconnect device cleanup is the fallback; the 250 ms in-process guarantee does not survive process/OS suspension.

The native output bridge is local TCP with no application input queue and TCP_NODELAY. Partial writes/congestion fail the session rather than corrupting 20-byte report framing. Remote traffic uses UDP ICE + DTLS/SCTP. TCP is only used for the local output bridge and signaling.

## Wire v1

| Offset | Length | Value |
|---|---|---|
| 0 | 3 | HP + version 1 |
| 3 | 1 | Zero-based logical slot 0–3 |
| 4 | 16 | Session UUID |
| 20 | 4 | Controller ID, little endian |
| 24 | 8 | Monotonic sequence |
| 32 | 8 | Sender monotonic microseconds (not wall clock) |
| 40 | 4 | Standard button mask |
| 44 | 8 | LX / LY / RX / RY, signed 16-bit SDL axes |
| 52 | 4 | LT / RT, 0–32767 |

SDL Y is positive down; Xbox output flips Y with saturating negation. SDL triggers retain 15-bit precision over the network and are rounded to the Xbox protocol's 8-bit range at the final backend only. SDL's standard south/east/west/north buttons map by position across Xbox/PlayStation labels.

## Trust and observation

The invite holds a room UUID and 244 random key bits (two independently generated v4 UUID payloads). The server receives SHA256(key), not the key. SDP and ICE messages include role and sequence under HMAC-SHA256, authenticating negotiated DTLS fingerprints against signal substitution by a server without the invite. Anyone holding the complete invite can join; there are no account identities in this alpha.

Creating a VIIPER API device does not establish OS compatibility. The backend requires a previously empty requested XInput index to appear, then independently compares a real non-neutral report read by XInput. `game_verified` remains false: only a recorded physical/game acceptance can establish that property. There is no invented connected controller, FPS, bandwidth, loss or latency in the UI.

## Boundaries

One guest / one active remote controller. Four logical slots are implemented in the receiver, not four concurrently implemented remote guests. Windows controls actual XInput enumeration order; arbitrary physical player-index reassignment is not implemented. macOS physical event handling, signed distribution and host emulation need Mac validation. Native media capture, GPU codecs, presentation, audio and keyboard/mouse are not implemented; no CPU screenshot renderer is substituted.
