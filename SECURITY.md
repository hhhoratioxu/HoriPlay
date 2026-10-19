# Security

This alpha uses WebRTC DTLS/SCTP, invite-authenticated SDP/ICE and explicit room/controller permissions. It is not an independent security audit.

Treat the entire invite as a bearer secret. Share it only with the intended participant. The server stores a join hash, and the invite expires for new joins after 10 minutes. Keep Internet signaling behind TLS; the app rejects remote plain WebSocket URLs. Accounts, identity-provider authentication and durable room state are not implemented.

VIIPER is a local trusted process boundary. Bind API/USBIP listeners to loopback and use a version you have reviewed. Current HoriPlay uses the documented optional unauthenticated localhost feeder mode. Do not expose it to the Internet. Local malware with equivalent user privileges is outside this alpha's trust boundary. No kernel installation or operating-system security bypass is performed.

Incoming state is bounded, validated, tied to a session/controller/slot grant and checked for increasing sequence. The watchdog requires fresh accepted states. It is an in-process guard and cannot guarantee a 250 ms release if Windows suspends or terminates the entire process. VIIPER's device-disconnect cleanup is the independent fallback; physically test that policy.

The app has no shell/remote-file execution command. Tauri custom commands are local to its bundled UI and its content policy blocks arbitrary remote content. TURN credentials are not persisted. CI uses scoped repository-provided credentials and no signing keys are committed. Installers are unsigned alpha artifacts, not silently downloaded updates.

Report potential vulnerabilities privately through GitHub private vulnerability reporting when available. Do not publish live invites, credentials or sensitive host captures in public issues.
