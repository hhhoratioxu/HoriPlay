# Third-party components

HoriPlay-owned code is MIT. Dependency licenses remain their own.

| Component | License / role | Distribution |
|---|---|---|
| SDL3 3.4.18 | zlib, physical input | Official checksum-pinned runtime in installers/portable tools; license below |
| Tauri 2 | MIT / Apache-2.0 | Native application shell |
| Microsoft WebView2Loader 1.0.3800.47 | BSD-3-Clause, Microsoft notice | Microsoft-signed x64 loader beside the GNU portable preview; [SDK license](docs/licenses/WebView2-SDK-LICENSE.txt) |
| React, TypeScript, Tailwind, Lucide | MIT / applicable package notices | UI dependencies in lockfile |
| webrtc-rs 0.14 | MIT / Apache-2.0 | ICE/DTLS/SCTP implementation, pinned by Cargo.lock |
| VIIPER standalone | GPL-3.0-or-later | Independently installed by user, not bundled or linked |
| usbip-win2 | BSD-2-Clause | User-approved separate signed driver, not bundled/installed |

VIIPER's upstream [license guidance](https://github.com/Alia5/VIIPER/blob/main/docs/libviiper/overview.md) explicitly distinguishes GPL libVIIPER linking from the standalone TCP feeder API. This repository implements the documented loopback protocol independently and does not copy or link its server source. Any future redistribution of the VIIPER executable must include the applicable source/notice obligations and be reviewed separately.

SDL3 license:

Copyright (C) 1997-2026 Sam Lantinga <slouken@libsdl.org>

This software is provided 'as-is', without any express or implied warranty.
In no event will the authors be held liable for any damages arising from
the use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not
   claim that you wrote the original software. If you use this software
   in a product, an acknowledgment in the product documentation would be
   appreciated but is not required.
2. Altered source versions must be plainly marked as such, and must not be
   misrepresented as being the original software.
3. This notice may not be removed or altered from any source distribution.
