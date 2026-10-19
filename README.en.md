# HoriPlay

**Native remote-controller engineering alpha · v0.1.0-alpha.1**

[简体中文](README.md) · [Setup](docs/GETTING-STARTED.md) · [Validation](docs/VALIDATION.md)

[Download v0.1.0-alpha.1](https://github.com/hhhoratioxu/HoriPlay/releases/tag/v0.1.0-alpha.1): Windows EXE/MSI, Apple Silicon/Intel DMGs, native diagnostic tools, SHA-256 sums and full-commit build records. Application installers are unsigned and Mac DMGs are not notarized. Only the Windows EXE has a recorded installation/startup smoke.

HoriPlay targets Windows/macOS remote game streaming and local multiplayer through independent remote controllers. **This release is an input-chain alpha. Video/audio streaming and real two-player game acceptance are incomplete. It is not yet a playable remote-streaming product.**

The implemented chain is SDL3 physical input → versioned analog-preserving reports → authenticated WebRTC DTLS/SCTP → host permission/sequence checks → standalone VIIPER Xbox 360 backend → independent Windows XInput observation. Game compatibility is a separate, manually recorded acceptance result; the UI never infers it from network connectivity.

The native desktop is Tauri 2 / React / TypeScript / Tailwind, with seven pages, light/dark themes, Chinese/English labels, live controller testing, radial dead zones, response curves, remapping, slot grants, rumble testing, selected ICE pair and measured timing percentiles. Browser previews cannot read native hardware and say so.

Two data channels carry unordered zero-retransmit full snapshots and reliable ordered button-edge snapshots, device lifecycle and permissions. Sessions use expiring private invites, explicit host approval, HMAC-authenticated SDP/ICE, WSS for remote signaling, bounded queues, sequence rejection, and a 250 ms fresh-input watchdog.

Windows virtual input requires an **independently installed, explicitly approved** signed USB/IP driver and running VIIPER process. No driver is installed by HoriPlay. No ViGEmBus dependency, unsigned-driver bypass, keyboard substitute or anti-cheat bypass is included. VIIPER is an independent GPL process; HoriPlay does not link libVIIPER.

## Build and test

Rust stable and Node.js 22+ are required. Windows packaging uses MSVC and Windows SDK.

```powershell
.\scripts\fetch-sdl.ps1
$env:HORIPLAY_SDL3 = (Resolve-Path .\runtime\SDL3.dll).Path
cargo test --locked
cargo run -p horiplay-cli -- gamepad-test --seconds 15
cargo run -p horiplay-cli -- xinput-test --seconds 15
cargo run -p horiplay-cli -- transport-bench --samples 300
cargo run -p horiplay-signaling
# another terminal
cd apps/desktop
npm ci
npm run tauri dev
```

For macOS, build the pinned SDL runtime with `bash scripts/fetch-sdl-macos.sh arm64` (or `x86_64` on Intel), then set `HORIPLAY_SDL3` to its absolute path. SDL3 guests compile on both architectures; physical-Mac acceptance is still required. macOS host virtual input is unavailable.

The core workspace excludes desktop GUI dependencies from default test targets; build the application using Tauri or `cargo build -p horiplay-desktop`. Windows GNU users in non-ASCII source directories should set `CARGO_TARGET_DIR` to an ASCII path.

## Honest limits

One host and one guest controller are supported by the session actor. Four logical slots are collision checked, but Windows owns XInput enumeration order; a mismatching requested slot fails instead of being relabeled. Rumble is wired through the backend and secure channel but is not hardware-accepted.

No native video/audio, keyboard/mouse injection, FPS claim, WAN acceptance, automatic ICE restart, multi-guest mode, macOS host gamepad, notarization, production signing or enabled updater is claimed. Automated protocol tests do not replace physical game/controller tests. See [the acceptance matrix](docs/VALIDATION.md) and [the next milestones](docs/ROADMAP.md).

HoriPlay-owned source is MIT licensed. See [third-party notices](THIRD-PARTY.md).
