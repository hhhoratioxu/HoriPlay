# Run the controller input alpha

This alpha does not transmit game video or audio. Use it to verify the real remote-controller chain before integrating native media.

## Windows host prerequisite

1. Obtain the signed [usbip-win2 installer](https://github.com/vadimgrn/usbip-win2/releases) from its upstream and inspect its digital signature. Installing a driver requires **your explicit approval**. HoriPlay will not invoke the installer or change Windows signing settings.
2. Obtain/build the standalone [VIIPER server](https://github.com/Alia5/VIIPER) for Windows and run it with its API and USB/IP listeners bound to loopback. This backend targets the API documented at commit `20aa112c6ea47187526dd88d0fd666a68a5f4416`. Use `viiper server --help` to confirm flag names for the version you install. The default API port is 3242; USB/IP is 3241. Local auto-attach must be enabled. HoriPlay's current feeder uses the upstream optional no-auth localhost mode; deployments requiring localhost authentication are not supported by this alpha.
3. Connect the host's physical controller first. Run `horiplay xinput-test --seconds 15` to see occupied system indices. Select the next free slot for the guest. Windows, rather than the HoriPlay label, controls assignment.
4. Place SDL3 beside the portable CLI or use an absolute `HORIPLAY_SDL3` path. Installers bundle the pinned official SDL runtime. No physical pad is simulated when hardware is absent.

## Signaling and TURN

For same-machine development, run `cargo run -p horiplay-signaling`; the default client URL is `ws://127.0.0.1:8787/ws`. For separate machines, deploy the service behind a WebSocket-compatible TLS reverse proxy and configure `wss://your-host/ws` on both endpoints. Plain `ws://` remote addresses are rejected.

The Linux Docker example is `deploy/compose.yml`. Copy `deploy/.env.example` to `deploy/.env`, supply your actual public address and strong TURN credentials, then run `docker compose --env-file deploy/.env -f deploy/compose.yml up --build -d`. The example binds signaling to server loopback for a TLS proxy. Forward UDP 3478 and 49160–49200 for coturn. Configure external/public addressing correctly if the server is itself behind NAT. This example is not an automatically provisioned cloud service.

Enter the STUN/TURN URLs and credentials in Settings **before** starting a room. TURN credentials stay in application memory. CLI users can pass repeated `--ice` URLs and use `HORIPLAY_TURN_USER` / `HORIPLAY_TURN_PASSWORD`. Never put a private key, token or production credential into a repository.

The app shows the actual selected ICE pair. A relay candidate is labeled TURN Relay. A STUN response alone is not connection proof. The benchmark uses actual peer traffic but runs on one machine, so it is not evidence of cross-network hole punching.

## Desktop sequence

1. Windows host: Create room. Privately share the complete invite. New joins expire after 10 minutes.
2. Windows/macOS guest: connect a physical controller, open HoriPlay, configure the same WSS service, then Join with the invite.
3. Host: verify the requested guest, then Allow connection.
4. Guest: choose the controller in Controllers. Its name and ID are announced over the encrypted reliable channel.
5. Host: choose the requested free Controller 1–4 index and Grant input. Creating a virtual device fails safely if the backend or the actual requested XInput slot is absent.
6. Run `horiplay xinput-test --seconds 30` independently on the host. Compare both controllers, button presses/releases, diagonals, 20/50/100% sticks and proportional triggers.
7. Only after independent detection, test a legally installed local-multiplayer game. Record the exact game/version, two devices, connection paths and outcome in the hardware evidence template.
8. Revoke remote input or End session at any time. A missing fresh report for 250 ms neutralizes the in-process backend. Test this physically before accepting it for gameplay.

CLI equivalent: `horiplay host --signaling wss://your-host/ws` and `horiplay join <invite> --signaling wss://your-host/ws`. On the host's stdin use `accept`, then `grant <reported-controller-id> 2`. Use `status`, `revoke`, and `stop` to inspect/control the session.

## Diagnostics

- `horiplay doctor`: runtime path, platform and actual occupied XInput indices.
- `horiplay gamepad-test --seconds 30`: native SDL3 enumeration and changing device states, no generated inputs.
- `horiplay xinput-test --seconds 30`: independent system observer.
- `horiplay transport-bench --samples 300`: synthetic 56-byte reports across actual WebRTC on the same PC. Reports RTT P50/P95/P99, not physical input-to-game latency.

The UI distinguishes raw SDL values, transmitted settings and observed system output. Reported work-time distributions exclude physical sampling/report intervals. There is no FPS/codec number until a real media pipeline is implemented and measured.
