# Validation evidence — 2026-10-19

This is a controller-input alpha. Automated protocol checks and successful compilation are not physical-controller, game or installation acceptance.

## Local automated checks

Environment: Windows x64, Rust 1.97.1 GNU (MSVC linker unavailable), Node.js 24.18.0. Source remains in the requested Chinese workspace; GNU build output uses `D:\CodeX\Builds\HoriPlay` to avoid dlltool Unicode-path failure.

- Rust core/protocol/session test suite: **14 tests passed** in the first complete run. Includes real ICE/DTLS/SCTP dual-channel data, actual forced TURN with a locally running Rust TURN server, approval-controlled signaling, codec boundaries, preserved analog values, replay/slot authorization, watchdog expiration and Xbox output conversion.
- `cargo check -p horiplay-desktop --locked`: **passed** on Windows GNU.
- `npm run build`: **passed** (TypeScript and production Vite build).
- Native application linking, final lint, installers and measured benchmarks: final outcomes are recorded in follow-up evidence below and GitHub workflow logs; do not infer them from the first check.

The unordered channel test repeats snapshots after channel-open negotiation: a zero-retransmit channel does not guarantee receipt of its first packet. Production continuously transmits fresh snapshots and sends button edges reliably. No test is allowed to call a lossy channel lossless.

## Physical acceptance matrix

| Test | Actual result |
|---|---|
| 1. Physical Xbox / DualSense / Edge / DS4 / Switch Pro USB and Bluetooth | Not performed; no physical device is declared verified |
| 2. Physical guest → network → Windows independent XInput | Not performed; requires user-approved signed driver and physical controller |
| 3. Host physical + guest virtual, independent indices | Not performed |
| 4. Real local two-player game | Not performed; no game name/result invented |
| 5. Two different networks / CGNAT / IPv6 | Not performed; automated same-machine direct and TURN are separate |
| 6. Held stick then real cable/network loss | Not performed; automated watchdog/permission tests passed separately |
| 7. Real 1080p60 / 120 FPS | Not implemented; no FPS data |
| 8. Windows Host + physical Windows/macOS Guest | Not performed |
| 9. Remote physical rumble | Not performed; no hardware rumble support claim |
| 10. 30-minute physical streaming/control | Not performed; video/audio not implemented |

Use [hardware-evidence.example.json](../tests/hardware-evidence.example.json) for real runs. Empty fields and `not_run` remain empty; automated protocol tests cannot populate a physical game pass.

## Model support

SDL3 mappings and native OS support are used for Xbox One, Xbox Series, DualShock 4, DualSense, DualSense Edge, Switch Pro and compatible third-party controllers. These are implementation targets. **The hardware-verified list is currently empty.** Optional rumble/battery/connection properties are queried rather than inferred from the name. Advanced triggers, motion, touchpads and HD haptics are not implemented.

## Release and performance records

CI and installer workflows are separate from runtime acceptance. Release BUILD files name the full source commit and hash every installer/tool archive. Unsigned application packaging does not mean the separately installed driver is unsigned; it also does not mean installation was tested. Local work-time/RTT numbers are appended only after executing the tools.

### Executed local follow-up

- Expanded suite: **15 tests passed**, including a 50-report regression that catches DCEP-handshake bytes starving the fast channel. Default-target strict Clippy passed.
- Native Windows GNU debug desktop and CLI linked. A 3-second startup smoke observed `HoriPlay · Controller Alpha`, a live process and `Responding=true`; the app was then stopped. This is startup evidence, not full UI or game acceptance. GNU linker emitted a multiple-resource-manifest warning; official installers use MSVC runners.
- Official SDL 3.4.18 archive was SHA256 verified, loaded, and polled for 3 seconds: **0 connected controllers**. Independent XInput polling also found **0 of 4 indices occupied**. Thus no physical device is hardware-verified.
- SDL poll-work samples on the empty device list: N=682, P50 **0.0119 ms**, P95 **0.1213 ms**, P99 **0.1489 ms**. These numbers exclude the poll interval and are not real controller sampling latency.
- Actual WebRTC DTLS/SCTP benchmark with synthetic 56-byte reports over explicit 127.0.0.1 ICE candidates: **300/300 completed**, 0 timed out, 0 congestion skips; RTT P50 **0.8281 ms**, P95 **1.0076 ms**, P99 **1.2142 ms**. See [raw benchmark JSON](evidence/webrtc-loopback-windows.json). This excludes physical input, virtual driver, game and WAN.
- One development benchmark using all host adapters timed out during ICE establishment on this multi-interface PC. The benchmark was isolated to actual loopback candidates so its scope is repeatable and explicit; production P2P still gathers normal host/STUN/TURN candidates. Cross-interface/WAN stability remains unaccepted.
- Controller UI preview was visually checked at the native 1240×820 viewport, with no horizontal overflow, Chinese/English and dark/light switching. Empty network metrics remained unmeasured. The saved screenshot is a browser preview with the native-unavailable notice; it is not a fabricated connected session.
- Official USBip 0.9.8.1 x64 installer was downloaded only for review. Windows Authenticode reported **Valid**, signer **Cloudyne Systems (Scheibling Consulting AB)**, SHA256 `38cad6d4432b52d5bb9409d9ad03b72fdffc4ada4cd3a48fbeca1a2752a8518a`. This verifies the installer signature only. No driver installation, contained-driver verification or restart was performed.

The first macOS CI desktop check failed because its bundled SDL resource was not prepared. CI was corrected to build the checksum-pinned runtime before checking the desktop; the failure was not counted as a passing Mac build.

### Native input runtime lifetime check

After the SDL main-thread lifetime change, the actual Windows `engine-test --seconds 2` completed with `input_error=null` and an empty real device list. The application worker collected 460 samples: poll-work P50 0.0093 ms / P95 0.0155 ms / P99 0.0248 ms. These empty-device work times are distinct from the earlier direct-poll run and do not measure physical input latency.

Windows and macOS CI at commit `96d5e7db89e2636275d15c64288a1430e705378d` both compiled and executed that native engine command, with `input_error=null` and `devices=[]`. The public run is [39260065889](https://github.com/hhhoratioxu/HoriPlay/actions/runs/39260065889). This validates library loading, worker polling and lifetime cleanup on the runner, without claiming physical hardware acceptance.

The first Windows installer build compiled the MSVC executable and NSIS setup, then failed on MSI's numeric prerelease-version restriction. The Windows config now maps `0.1.0-alpha.1` to MSI version `0.1.0.1`. The unpublished build was restarted after clearing rumble/freshness state for each new controller grant; final artifacts must name the corrected source commit in their BUILD records.

The coturn deployment sample was statically checked against the 4.6.3 upstream options. The removed `no-loopback-peers` option is omitted; the upstream default denies loopback peers. Docker Compose was not executed on this PC because Docker is unavailable.

## Published alpha verification

- Actual public, non-draft prerelease: [v0.1.0-alpha.1](https://github.com/hhhoratioxu/HoriPlay/releases/tag/v0.1.0-alpha.1).
- Exact release source/tag: `e360f6a60439b429221e279a1f1a02a2d5e539b2`. All three BUILD records and the GitHub tag/Release target agree.
- [Final CI](https://github.com/hhhoratioxu/HoriPlay/actions/runs/39261170994): Windows and macOS passed all 15 protocol/core tests, strict Clippy, desktop check, frontend build and actual native SDL engine loading.
- [Final installer/release workflow](https://github.com/hhhoratioxu/HoriPlay/actions/runs/39261225037): Windows x64 MSVC EXE + MSI, aarch64 macOS DMG, x86_64 macOS DMG and all portable diagnostic tools succeeded. Runner Rust was 1.99.1. Thirteen assets were actually published.
- All 13 assets were downloaded from the published Release. The seven installer/tool archives passed local nonempty/size/SHA256 checks against their BUILD files. [Local checksum results](evidence/release-checks.json).
- Mac tool archives were extracted and their CLI, signaling server and SDL dylib Mach-O CPU fields independently checked: ARM64 `16777228` and x86_64 `16777223`. This checks architecture, without claiming a physical Mac installation.
- Windows NSIS EXE installed successfully with exit code 0 into `D:\CodeX\Builds\HoriPlay\installed-alpha`. The executable and bundled SDL3 were present. With the dev server stopped, a four-second startup smoke observed the expected titled window, `Responding=true` and no early process exit. The smoke process was then stopped. [Startup record](evidence/installed-windows-startup.json). Full native UI interaction/gameplay was not accepted by this check.
- Published portable Windows tools loaded their bundled SDL3 without a source-tree environment override. Native engine smoke completed with `input_error=null` and `devices=[]`; independent XInput saw no occupied controller indices. [Native engine record](evidence/native-engine-windows-release.json).
- Published optimized Windows CLI: **300/300** actual WebRTC synthetic-input round trips completed, 0 timeout / 0 congestion skips, RTT **P50 0.2783 ms / P95 0.4068 ms / P99 0.6778 ms**. [Raw release benchmark](evidence/webrtc-loopback-windows-release.json). These are same-machine 56-byte DTLS/SCTP round trips, not physical input-to-game, WAN, capture-to-display or FPS measurements.

MSI installation and physical Mac DMG installation are untested. The application is unsigned/unnotarized, virtual-driver installation still awaits explicit user approval, and all physical/game/media rows above remain `not performed`. Deployment documentation on main includes the post-tag coturn sample correction; packaged application binaries remain pinned to the release source above.
