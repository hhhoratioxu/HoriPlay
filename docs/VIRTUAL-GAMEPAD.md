# Virtual controller decision record — 2026-10-19

## Windows

The initial backend is an independent, user-run [VIIPER](https://github.com/Alia5/VIIPER) server with its documented Xbox 360 feeder protocol. Research snapshot: upstream commit `20aa112c6ea47187526dd88d0fd666a68a5f4416`. Management uses null-terminated JSON requests on loopback port 3242; the device stream carries fixed 20-byte input and 2-byte dual-motor feedback. See upstream [API](https://github.com/Alia5/VIIPER/blob/20aa112c6ea47187526dd88d0fd666a68a5f4416/docs/api/overview.md) and [Xbox format](https://github.com/Alia5/VIIPER/blob/20aa112c6ea47187526dd88d0fd666a68a5f4416/docs/devices/xbox360.md).

The kernel prerequisite is [usbip-win2](https://github.com/vadimgrn/usbip-win2), whose upstream describes signed Windows drivers. The observed latest release at investigation was `v.0.9.8.1`. This is an upstream capability claim, **not verification that a driver is installed, signed or compatible on this PC**. HoriPlay never installs it. Users must obtain it from the upstream, verify its signature, approve installation themselves, and restart if its installer requires it.

HoriPlay creates a dedicated bus, opens the stream within VIIPER's cleanup deadline and removes that bus on cleanup. API creation is insufficient: the requested empty XInput index must appear within 3 seconds or the backend removes its device and refuses the grant. Actual non-neutral reports are compared through an independent XInput reader. A game that sees a controller is a further hardware/game test.

There is no arbitrary XInput index-setting API here. Connect the host's physical pad first for common Player 1 / Player 2 order. Select a free requested slot in HoriPlay. If Windows assigns a different index, the grant fails visibly. Do not relabel a mismatched OS index to pretend slot assignment succeeded.

| Alternative | Finding | Decision |
|---|---|---|
| [ViGEmBus](https://docs.nefarius.at/projects/ViGEm/End-of-Life/) | Permanently retired | No dependency or automatic install |
| [VirtualPad](https://docs.nefarius.at/projects/VirtualPad/) | Commercial business-partner framework | No SDK/license is available in this workspace; not falsely integrated |
| [Microsoft VHF](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/virtual-hid-framework--vhf-) | HID source driver runs in kernel mode; generic HID is not automatically XInput | Possible independent driver project after compatibility and signing work |
| Independent USB emulation driver | WDK/HLK and production signing lifecycle needed | Not created as an unsigned placeholder |

If an in-house driver is required later, it must be a separate least-privilege KMDF/UdeCx project, with validated device reports, restricted IOCTL access, disconnect-neutral semantics and a clear uninstall path. Test certificates are only for controlled test machines. [Microsoft production signing](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/windows-driver-signing-tutorial) is a separate release gate. HoriPlay will not disable Secure Boot, signature enforcement or install unsigned kernel code.

## macOS

[GameController](https://developer.apple.com/documentation/gamecontroller) supports reading physical controllers; SDL's native backend uses platform APIs. This does not mean HoriPlay can publish a system-wide Xbox controller to a third-party game.

[CoreHID](https://developer.apple.com/documentation/corehid) now documents virtual HID creation. Investigate [Creating virtual devices](https://developer.apple.com/documentation/corehid/creatingvirtualdevices) and [the virtual HID entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.hid.virtual.device) using a current SDK and signed test app. Do not infer broad game compatibility or entitlement approval from API availability.

The older IOHIDUserDevice route also requires investigation of current permissions, sandboxing and distribution restrictions. A generic HID device may not be exposed as a GameController profile accepted by the target games. [HIDDriverKit / DriverKit](https://developer.apple.com/documentation/driverkit) uses signed system extensions and requested entitlements. No SIP/AMFI bypass or entitlement spoof is acceptable.

The maintainable macOS plan is: entitled CoreHID prototype → independent HID enumeration → GameController/SDL visibility → real native game → signed and notarized distribution on Apple Silicon and Intel. DriverKit is evaluated only if CoreHID cannot meet compatibility requirements. No macOS host emulation is marked implemented. Windows Host + macOS Guest remains the first cross-platform target.
