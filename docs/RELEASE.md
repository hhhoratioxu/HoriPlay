HoriPlay v0.1.0-alpha.1 is a **controller-input engineering alpha**, not a complete playable remote-game streaming release.

Implemented: native SDL3 input and diagnostic UI, analog-preserving input reports, encrypted native WebRTC dual channels, expiring rooms and explicit host/controller approval, Windows standalone VIIPER backend with independent XInput checks, disconnect neutralization and rumble plumbing.

Installers and portable tools are built in release profile on GitHub runners. Each platform includes SHA-256 sums and full-commit BUILD provenance. The application installers are **unsigned**; macOS DMGs are **not notarized**. Building an installer is not proof of successful installation or physical-device/game compatibility.

The Windows virtual-controller prerequisite is a separately obtained, user-approved signed usbip-win2 driver and a running VIIPER server. HoriPlay does not install, bypass signing for, or bundle a driver.

**Incomplete:** native video/audio, real two-controller local-multiplayer acceptance, physical Windows/macOS guest acceptance, cross-network direct/TURN acceptance, 1080p60 / 120 FPS, 30-minute physical streaming stability, ICE restart/reconnect, multiple guests, macOS host virtual input and signed updates. No physical controller model is marked hardware-verified by this release.

Read the repository's validation matrix and getting-started guide before attempting the input alpha. Keep production signing and device/game acceptance as separate release gates.
