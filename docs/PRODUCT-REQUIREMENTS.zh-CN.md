# HoriPlay — Windows / macOS 超低延迟 P2P 远程游戏串流与手柄同玩软件

## 你的角色

你是一名具有实际产品交付经验的资深 Rust、C++、Windows 内核驱动、macOS 原生开发、WebRTC、实时网络传输、GPU 编解码和游戏手柄输入工程师。

请开发一款完整的跨平台远程游戏软件：

**项目名称：HoriPlay**

GitHub 仓库：

https://github.com/hhhoratioxu/HoriPlay

参考 Parsec、Moonlight、Sunshine 的远程游戏体验，但必须独立实现自己的应用架构和用户界面。

最终软件需要支持 Windows 和 macOS。

这不是网页演示，也不是一个只会显示远程屏幕的软件。

**最重要的验收要求：用户和朋友可以安装 HoriPlay，使用各自的真实游戏手柄，远程控制同一台主机上的游戏。游戏必须能够真正识别远程手柄。**

不要只实现手柄按键检测或在界面里显示一个手柄图标。

---

# 一、核心使用场景

假设：

用户 A：Windows 电脑，运行游戏。

用户 B：macOS 或 Windows 电脑，在另一个网络环境。

双方都安装 HoriPlay。

1. A 启动 HoriPlay 并创建远程游戏房间。
2. B 启动 HoriPlay，输入邀请码加入房间。
3. 双方通过 ICE/STUN 进行 P2P 打洞。
4. 优先建立加密 UDP 直连。
5. A 的游戏视频与声音实时传给 B。
6. B 使用自己电脑连接的 Xbox、DualSense 或其他手柄进行操作。
7. B 的手柄输入通过低延迟网络通道传给 A。
8. A 的操作系统将 B 的输入呈现为游戏能够识别的独立控制器。
9. A 使用自己的手柄作为玩家 1。
10. B 使用自己的手柄作为玩家 2。
11. 双方可以玩同一台主机上的双人游戏。

必须支持以下模式：

- 单人远程游戏。
- 双人远程合作游戏。
- 主机键盘 + 访客手柄。
- 主机手柄 + 访客手柄。
- 访客独占手柄控制。
- 主机和访客各自使用独立手柄。
- 主机主动选择远程玩家的控制器编号。
- 主机随时撤销远程控制权限。

游戏是否真正支持双人同玩，取决于游戏本身是否具备本地多人模式。

---

# 二、手柄支持是 P0 级核心功能

这是最高优先级需求，必须优先开发和验收。

## 2.1 支持的手柄

至少支持：

- Xbox One Controller
- Xbox Series X/S Controller
- PlayStation DualShock 4
- PlayStation DualSense
- PlayStation DualSense Edge
- Nintendo Switch Pro Controller
- 常见兼容 XInput 的第三方手柄

支持 USB 有线和蓝牙连接。

必须实现手柄即插即用。

用户在启动软件后才插入手柄，也应该自动识别。

## 2.2 本地手柄检测

使用 SDL3 Gamepad API，并结合系统原生接口。

Windows：

- XInput
- Windows.Gaming.Input（适用时）
- SDL3

macOS：

- Apple GameController Framework
- SDL3

建立统一的 Gamepad Abstraction Layer。

必须能识别：

- 手柄名称
- 厂商与设备标识
- 连接状态
- 电池状态（如果系统支持）
- 所有按钮
- 左右摇杆
- 左右扳机
- D-Pad
- 手柄功能能力

不能仅根据设备名称假定震动、陀螺仪或其他可选功能存在。

## 2.3 完整的按键传输

必须支持：

- A / B / X / Y
- Cross / Circle / Square / Triangle
- LB / RB
- LT / RT
- L3 / R3
- Start / Menu
- Select / View
- D-Pad 四方向
- 左摇杆 X/Y
- 右摇杆 X/Y
- 线性模拟扳机
- 同时按下多个按键

特别注意：

摇杆输入不能只表示上下左右。

必须保留实际模拟量。

例如：

左摇杆推动 20%、50%、100% 时，远程游戏应该得到相应的模拟轴值，而不是全部变成 100%。

LT/RT 必须保留模拟扳机行程。

## 2.4 远程虚拟手柄

这是核心中的核心。

当朋友通过 HoriPlay 连接到 Windows Host 时：

**Windows 上的游戏必须把朋友的远程手柄识别为一个真正独立的游戏控制器。**

不能只把按钮映射成键盘。

实现结构：

Guest Physical Controller
→ SDL3 / Native Input
→ Input Normalization
→ Low Latency Network
→ Host Input Receiver
→ Virtual Gamepad Backend
→ Windows Game Input
→ Running Game

Windows Host 优先提供兼容 XInput 游戏的虚拟控制器。

技术方案要求：

1. 调研目前仍在维护、具有适当许可证和签名支持的虚拟手柄解决方案。
2. 不得默认将已停止维护的 ViGEmBus 作为长期核心依赖。
3. 如确实需要自主驱动，单独设计符合 Windows 安全模型的签名驱动项目，明确测试签名和正式发布签名的区别。
4. 所有驱动安装必须由用户明确同意。
5. 不得绕过 Windows 驱动签名安全机制。
6. 不得通过不安全的内核修改或注入方式伪造支持。
7. 实际测试 Windows 游戏是否能够识别控制器并读取输入。

必须实现 Virtual Gamepad Backend 接口，方便日后替换底层驱动。

## 2.5 多手柄支持

设计至少支持四个逻辑手柄插槽：

- Controller 1
- Controller 2
- Controller 3
- Controller 4

用户可以手动配置：

| 玩家 | 控制器来源 | 主机插槽 |
|---|---|---|
| Player 1 | Host Xbox Controller | Slot 1 |
| Player 2 | Guest DualSense | Slot 2 |
| Player 3 | Guest Controller 2 | Slot 3 |
| Player 4 | 扩展访客 | Slot 4 |

第一阶段至少实现一名 Host + 一名 Guest 同时操作两个独立手柄。

多访客和四人远程同玩作为架构扩展目标。

必须避免两个用户意外占用同一个控制器插槽。

## 2.6 手柄震动

支持 Rumble Feedback。

传输流程：

Host Game
→ Supported Rumble Backend
→ Secure Network
→ Guest Device
→ Physical Controller

要求：

- 普通双马达震动
- 震动强度
- 震动持续时间
- 震动停止
- 不支持时自动关闭

DualSense 自适应扳机、HD Haptics、陀螺仪、触摸板作为增强功能，但基础版本不允许虚假宣称支持。

必须先测试 Host 虚拟手柄后端是否能获得游戏震动命令。

如果后端不支持震动回传，要显示真实状态。

## 2.7 手柄设置界面

创建专门的 Controller Settings 页面。

功能：

- 检测手柄
- 手柄实时按键测试
- 摇杆可视化
- 扳机行程可视化
- Dead Zone 调整
- 摇杆曲线
- 灵敏度
- 手柄按钮映射
- Xbox / PlayStation 按钮显示方案
- 控制器插槽分配
- 震动测试
- 设备断线提醒

在远程会话期间显示：

**Guest Controller → Connected → Virtual Controller 2 → Game Input Active**

每个状态都必须由实际检测结果决定。

不能单凭网络连接成功就显示 Game Input Active。

---

# 三、手柄传输必须低延迟

手柄通信不能使用普通高延迟业务队列。

优先使用 WebRTC DataChannel，针对不同输入事件选择合适的传输语义。

设计：

### 高频状态通道

传输：

- 摇杆当前位置
- 模拟扳机
- 高频连续输入

优先使用无序、低重传或不可靠传输，避免旧状态阻塞新状态。

数据包包含：

- Session ID
- Controller ID
- Sequence Number
- Timestamp
- Button Bitmask
- Analog Axes
- Trigger Values

### 关键事件通道

必须保障：

- Button Press
- Button Release
- Controller Connect
- Controller Disconnect
- Controller Slot Change

可采用可靠有序传输，结合状态快照、序列号及去重处理。

防止：

- 按钮卡住
- 按键状态丢失
- 摇杆停留在旧位置
- 断线后角色一直向前跑

断线后必须自动将远程虚拟控制器恢复到安全中立状态。

### 延迟目标

持续测量：

- Controller Input Sampling
- Input Serialization
- Network Transit
- Host Input Dispatch
- Virtual Device Update

优先低延迟而不是追求过高的手柄采样频率。

不得用固定 1000Hz 轮询掩盖实际输入设备或系统接口的限制。

提供输入延迟 P50 / P95 / P99 测试结果。

---

# 四、P2P 网络系统

目标是最大化真实 P2P 直连概率，并最小化网络延迟。

技术要求：

- ICE
- STUN
- TURN
- UDP Hole Punching
- Trickle ICE
- IPv4
- IPv6
- CGNAT 网络适配
- UDP 连接保持
- ICE Restart
- 断线自动重连
- NAT 与路径检测
- LAN 优先直连
- TURN Fallback

不得假设所有网络都能够打洞成功。

P2P 成功后视频、音频和控制信号优先点对点传输。

提供 Rust 信令服务器：

- Tokio
- Axum
- WebSocket
- SDP 交换
- ICE Candidate 交换
- 会话鉴权
- 用户授权
- 房间邀请码
- 心跳与连接状态

提供 Docker Compose 配置，包含信令服务器与 coturn 示例。

### 网络状态显示

显示：

- Local / LAN
- P2P Direct
- TURN Relay
- IPv4 / IPv6
- RTT
- Packet Loss
- Jitter
- Estimated Bandwidth

不得将 STUN 地址发现等同于 P2P 连接成功。

---

# 五、高帧率低延迟视频

必须使用原生 GPU 捕获、硬件编解码与 GPU 显示管线。

Windows：

- Windows Graphics Capture
- Direct3D 11
- DXGI Desktop Duplication（备用）
- NVIDIA NVENC
- AMD AMF
- Intel Quick Sync

macOS：

- ScreenCaptureKit
- Metal
- VideoToolbox

编码格式：

- H.264 必选
- HEVC 条件支持
- AV1 条件支持

视频配置：

- 720p 60 FPS
- 1080p 60 FPS
- 1080p 120 FPS
- 1080p 144 FPS
- 1440p 60/120 FPS
- 4K 60 FPS

实际运行前执行硬件能力检测。

不支持的分辨率、帧率和编码器不得假装支持。

视频码率：

5–100 Mbps 可配置。

实现：

- Adaptive Bitrate
- Congestion Control
- Frame Pacing
- Low Latency Jitter Buffer
- Video Frame Drop Policy
- Hardware Decoding
- GPU Rendering
- Fullscreen
- Multiple Monitors
- HDR/SDR 能力检测

低延迟模式中禁止无限积压旧视频帧。

---

# 六、Windows 与 macOS 跨平台

开发目标：

| Host | Guest | 目标 |
|---|---|---|
| Windows | Windows | 完整游戏串流 + 手柄 |
| Windows | macOS | 完整游戏串流 + 手柄 |
| macOS | Windows | 游戏串流 + 原生远程手柄能力验证 |
| macOS | macOS | 游戏串流 + 原生远程手柄能力验证 |

特别说明：

macOS 上读取真实手柄输入和向第三方游戏提供虚拟手柄输入是两个不同的能力。

目前不能假设 macOS 可以像 Windows 一样轻松安装通用虚拟 Xbox 手柄。

请先调查 Apple GameController、CoreHID、IOHIDUserDevice 和 DriverKit 的官方限制、权限与实际游戏兼容性。

如果无法以可维护、可签名、可实际分发的方式实现 macOS Host 虚拟手柄：

- Windows Host + macOS Guest 的远程手柄功能必须继续完整实现。
- macOS Host 的游戏串流功能可以单独交付。
- 不得将手柄映射键盘视为完整的原生手柄功能。
- 不得把未经测试的 macOS 虚拟控制器列为已完成。
- 必须明确记录限制和后续工程方案。

优先把 Windows Host 的双人手柄同玩交付到真正可用的状态。

---

# 七、音频

支持：

- Windows WASAPI Loopback
- macOS ScreenCaptureKit Audio
- Opus
- 48kHz
- 游戏音效串流
- 低延迟播放
- 音画同步
- 音量调整
- 静音
- 音频设备切换

后续增加语音聊天。

---

# 八、桌面界面

技术：

- Rust
- Tauri 2
- React
- TypeScript
- Tailwind CSS

高帧率视频处理不得依赖 WebView 中低效的 CPU 帧拷贝。

必须使用原生媒体播放或渲染后端。

设计：

- Liquid Glass 风格启发
- Fluent Design 风格启发
- 深色 / 浅色模式
- 简体中文 / 英文
- 高 DPI 支持
- 简洁流畅的动画

导航：

1. Home
2. My Devices
3. Host Game
4. Join Game
5. Controllers
6. Network
7. Settings

### Controller 页面

这是重点页面。

必须能够实时看到：

- 我的手柄
- 朋友的手柄
- 手柄按键状态
- 控制器编号
- 手柄与主机映射关系
- 虚拟手柄运行状态
- 输入传输延迟
- 震动能力
- 手柄连接方式
- 控制权限

### 游戏 Overlay

显示：

FPS / Bitrate / RTT / Packet Loss / Encode / Decode / Input Latency / Connection Type。

能够随时切换性能监控 Overlay。

---

# 九、安全设计

必须具备：

- 端到端加密的实时媒体和控制传输
- 显式连接授权
- 房间邀请码有效期
- 会话身份校验
- 独立控制器权限
- Host 一键踢出
- Host 一键停止串流
- 输入权限即时撤销
- 会话断开后释放远程按钮状态
- 不允许未授权输入注入
- 不允许绕过系统权限
- 不实现反作弊规避

所有手柄和虚拟设备功能只能用于用户主动批准的远程会话。

---

# 十、性能优先级

从高到低：

1. 手柄输入真实性和准确性
2. 输入到游戏的实际低延迟
3. 视频端到端低延迟
4. 稳定的 P2P 连接
5. 60/120 FPS 稳定输出
6. 画质与码率
7. UI 动画和附加功能

必须避免：

- 软件编码造成高延迟
- 不必要的 GPU/CPU 数据复制
- TCP 队头阻塞影响高频控制状态
- 大量输入消息堆积
- 视频帧积压
- 远程控制器状态不同步

不要仅通过 ping 判断游戏响应速度。

必须实际测试视频与游戏输入链路。

---

# 十一、真实验收测试

必须准备可运行的 Gamepad Test 工具。

### Test 1 — 手柄基础检测

连接 Xbox、DualSense 等手柄，确认按钮、摇杆和扳机可以实时识别。

### Test 2 — 远程手柄输入

朋友在客户端连接手柄。

主机必须识别虚拟控制器。

按下 A 键，主机测试工具能够检测到 A 键按下和释放。

移动摇杆，主机看到正确的模拟轴值。

### Test 3 — 双人同时操作

主机有一个真实手柄。

朋友通过网络连接一个真实手柄。

Windows 上的游戏或者独立输入测试程序必须同时识别两个不同的控制器。

两人的输入不能互相覆盖。

### Test 4 — 实际游戏

使用合法安装、支持本地双人的游戏进行真实测试。

确认两人能够同时操作两个角色。

必须记录游戏名称、设备配置、连接方式和测试结果。

### Test 5 — P2P

跨不同网络连接。

记录实际 ICE Candidate Pair，验证是否 P2P。

### Test 6 — 断线恢复

远程玩家按住摇杆，然后断开网络。

确认主机端的虚拟手柄能释放所有状态。

### Test 7 — 高帧率

验证 1080p60。

然后测试 1080p120。

不能只设置 FPS=120 就声称达到稳定 120 FPS。

### Test 8 — 跨平台

至少验证：

- Windows Host + Windows Guest
- Windows Host + macOS Guest

进一步验证 macOS Host 各项功能。

### Test 9 — 震动

若已实现震动回传，实际检查远程物理手柄是否震动。

### Test 10 — 长时间运行

执行至少 30 分钟连续串流及远程控制稳定性测试。

所有测试必须输出真实结果，区分自动测试与人工硬件验收。

---

# 十二、开发顺序

## Phase 1 — 工程初始化

建立 Rust Monorepo、Tauri 应用、架构文档和 CI。

## Phase 2 — Gamepad Core

优先实现：

- SDL3 手柄识别
- 完整手柄状态读取
- 手柄按钮和模拟轴
- 手柄测试 UI
- 输入序列化协议
- Windows 虚拟控制器后端可行性验证

这个阶段不能跳过。

## Phase 3 — P2P

建立真正可用的 ICE/STUN/TURN 网络连接。

让两台不同网络的电脑能够传输真实手柄状态。

## Phase 4 — Virtual Gamepad

Windows Host 创建可被游戏识别的远程虚拟控制器。

实现玩家 1 和玩家 2 的独立输入。

这是第一个重大交付里程碑。

## Phase 5 — Video Streaming

实现 Windows 原生屏幕捕获、GPU 编码、P2P 视频传输和 GPU 解码。

达到真实 1080p60。

## Phase 6 — Remote Play

整合：

- 视频
- 系统声音
- 键盘鼠标
- 远程游戏手柄
- 虚拟控制器
- 房间权限

完成 Windows Host + Windows/macOS Guest 的真实远程游戏。

## Phase 7 — Performance

优化：

- 1080p120
- 输入延迟
- GPU 负载
- 动态码率
- 画面稳定
- 自动恢复
- 震动回传

## Phase 8 — macOS Host

实现 macOS 原生捕获和编码。

评估并实现操作系统允许的原生远程手柄功能。

不得把模拟键盘当成完整虚拟手柄。

## Phase 9 — Release

生成安装包并发布 GitHub Release。

---

# 十三、GitHub 自动发布

使用：

https://github.com/hhhoratioxu/HoriPlay

要求：

- 完整源代码
- MIT License
- README（中英文）
- 文档
- 配置示例
- 自动测试
- GitHub Actions
- Windows Installer
- macOS Installer
- Release Notes
- 自动更新机制

发布格式：

Windows：
- `.exe`
- `.msi`

macOS：
- Apple Silicon `.dmg`
- Intel `.dmg`

版本从：

`v0.1.0-alpha.1`

开始。

不要把未完成的功能标记为稳定可用。

在操作 GitHub 前执行 `gh auth status`，确认身份和仓库权限。不存在的仓库可以创建，已有仓库不能直接覆盖。

自动发布不能泄露任何 Token 或签名证书。

---

# 十四、项目目录

建议：

```text
HoriPlay/
├── apps/
│   └── desktop/
├── crates/
│   ├── core/
│   ├── networking/
│   ├── gamepad/
│   ├── virtual-gamepad/
│   ├── input-transport/
│   ├── media/
│   ├── audio/
│   ├── windows/
│   └── macos/
├── services/
│   └── signaling/
├── deploy/
├── docs/
├── tests/
├── .github/
│   └── workflows/
├── Cargo.toml
├── README.md
└── LICENSE
```

允许根据实际依赖调整结构。

---

# 十五、绝对禁止的假实现

我不接受：

- 只有手柄 UI，没有实际手柄识别。
- 只能测试手柄，无法远程使用。
- 只把手柄映射为键盘，却宣称实现原生手柄。
- 主机游戏无法识别远程虚拟手柄。
- 只有一个输入通道，双人输入互相覆盖。
- 使用静态画面模拟串流。
- 只有信令连接，没有真实 P2P。
- 只有 Windows 代码，却声称 macOS 已完成。
- 未经测试就宣称 120 FPS。
- 未经测试就宣称超低延迟。
- 无法安装却宣称发布成功。
- 大量 Stub、TODO 或空模块冒充完成。

---

# 十六、最终验收标准

首个真正可玩的版本必须达到：

**一台 Windows 主机运行游戏。**

**另一个使用 Windows 或 macOS 的用户安装 HoriPlay。**

**朋友把 Xbox 或 PlayStation 手柄连接到自己的电脑。**

**两台电脑建立 P2P 连接，或者在直连失败时自动使用 TURN。**

**朋友能够看到实时游戏画面，听到游戏声音。**

**主机游戏能够实际识别朋友的远程手柄。**

**两个人能够使用不同的控制器共同游玩支持本地双人的游戏。**

**至少稳定实现 1080p60，之后进一步优化到 120 FPS。**

软件必须实际可玩，而不只是技术演示。

---

# 十七、立即开始执行

现在直接检查开发环境和现有 GitHub 仓库。

先建立可运行的工程骨架，并优先开发真正的手柄输入链路。

遇到方案选择时，按照以下原则决策：

**真实可用 > 手柄操作准确 > 低延迟 > 稳定性 > 高帧率 > 外观。**

不要先把几千行 UI 写完，再发现虚拟手柄无法实现。

优先打通：

Physical Controller → P2P Transport → Virtual Gamepad → Real Game。

随后整合实时视频和音频。

每个阶段必须编译、测试、修复问题，保持代码可以继续迭代。

如缺少其他平台硬件、签名证书或 GitHub 授权，明确报告限制，不得虚构测试或发布结果。

最终给出：

1. GitHub 仓库链接。
2. GitHub Release 链接（如已实际发布）。
3. Windows/macOS 构建结果。
4. 实际支持的手柄列表。
5. 远程虚拟控制器实现情况。
6. 双人同玩测试结果。
7. P2P 测试结果。
8. 延迟和 FPS 测试数据。
9. 尚未实现的功能。
10. 下一阶段的开发计划。

**现在开始开发 HoriPlay。优先实现可用的远程游戏手柄，最终交付真正可以与朋友一起玩游戏的软件。**