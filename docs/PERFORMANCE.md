# 输入性能优化实测 — 2026-10-20

本轮只优化现有输入链路。没有安装 USB/IP 驱动，没有新增视频或音频功能，也没有把合成测试当成实体手柄或真实游戏验收。已发布的 `v0.1.0-alpha.1` 安装包不包含本轮改动。

## 实际改动

- 输入变化直接通知会话发送，移除采样与网络发送之间独立的 8 ms 定时门槛。Windows 的定时唤醒会受系统调度影响，实际等待可能超过配置的 8 ms；下面记录的是测量值。
- 摇杆/扳机连续状态只保留最新一份。已采到的按键变化及连接变化在授权期间用最多 64 条的有界缓冲保序；缓冲溢出结束会话，避免静默遗漏松开事件。未授权期间不积压旧操作。
- 接收端也只保留最新的不可靠状态，可靠事件单独保序并优先处理。WebRTC 传入字节缓冲直接转移，省去一次数据复制。
- 手柄名称、VID/PID 在打开时缓存；电量、连接类型、能力每秒刷新。每次采样检查已有设备是否断开，新增设备约每 100 ms 枚举一次，另加采样周期和系统调度误差。
- 有手柄时维持 4 ms 的采样安排；无设备时退让到 16 ms。UI 设备快照约 30 Hz 更新，排序统计降至 10 Hz，并放在共享锁之外；计时窗口以固定容量、常数时间写入。
- 未变化状态在保活之间不重复发送；保活定时器为 50 ms，最小发送间隔为 50 ms，系统调度可能加大间隔。网络发送缓冲拥塞时只重试最新状态，重试安排为 8 ms，不等待空闲保活，也不恢复旧模拟量队列。超过 100 ms 的旧采样不能重新发送为保活。原有主机 250 ms 失效归零检查保留。

本地保留每个已采到的按键变化，不等于真实游戏已经收到每次短按：两条网络通道仍共享序号，较新的状态可能使延迟到达的旧事件失效。实体采样盲区、跨通道重排及真实游戏兼容性仍需要单独实测。

## 调度对照

Windows x64，Rust 1.97.1 GNU，`release` 优化构建，最终运行时代码提交 `d6b141b1d37a26ff2934b1cc8230d73c7b75f9a6`。增加拥塞重试保护后重新执行了下面全部六轮，CLI SHA256 为 `afcc8ed587dfdfeb9e542726306e50bace58ee2f6a2c77900bef9daf3a885014`。由独立原生线程产生明确标注的合成输入，经过生产代码的输入邮箱/发送器、真实 WebRTC ICE + DTLS + SCTP，再由接收端检查会话、权限、插槽及序号。两端都使用 127.0.0.1，32 个预热样本不计入结果。

两组运行**同一个可执行文件**，只切换“8 ms 定时发送”与“变化通知发送”。定时组是隔离旧调度方式的对照程序，并非旧版完整 Engine 的实测。每次输入都等待接收确认后才产生下一次输入，以免主动合并状态干扰这项调度测量。这也意味着本测试不代表持续满速输入、拥塞、丢包或 WAN 表现。发生样本超时会使整次命令失败，不会从分位数里隐去。

| 轮次 | 调度方式 | 完成样本 | 生成至授权接收 P50 | P95 | P99 |
|---|---|---:|---:|---:|---:|
| 1 | 8 ms 定时对照 | 600/600 | 10.6731 ms | 14.1921 ms | 14.3781 ms |
| 1 | 变化通知 | 600/600 | 0.1310 ms | 0.2278 ms | 0.2709 ms |
| 2 | 变化通知 | 600/600 | 0.1399 ms | 0.2443 ms | 0.5168 ms |
| 2 | 8 ms 定时对照 | 600/600 | 10.7073 ms | 14.2010 ms | 14.6546 ms |
| 3 | 8 ms 定时对照 | 600/600 | 10.7144 ms | 13.7748 ms | 14.3299 ms |
| 3 | 变化通知 | 600/600 | 0.1283 ms | 0.2180 ms | 0.2808 ms |

三轮各自分位数的中位值：P50 **10.7073 → 0.1310 ms**，P95 **14.1921 → 0.2278 ms**，P99 **14.3781 → 0.2808 ms**。调度对照的 P50 下降约 **98.8%**。每组 1,800 个计入统计的样本全部完成；不能把这个计数当成 Internet 零丢包证明。

原始命令输出、每轮范围与可执行文件 SHA256 见 [input-comparison.json](evidence/performance/input-comparison.json)。其中发送等待统计使用生产代码最近 1,024 次编码尝试的滚动窗口；可靠事件与最新状态可各记录一次。表格中的“生成至授权接收”每个输入只记录一次，包含每轮全部 600 个有效样本。

这些是**本机软件链路的单程耗时**，不包括真实 SDL 设备获取、USB/蓝牙、虚拟驱动、游戏处理、WAN、视频及音频。此前 Release 的 `transport-bench` 是网络往返时间，测量起止点不同，不能直接放在同一张表里比较。

## 原生空闲进程 CPU

旧版源码基线为本轮开始时的 `692a976`；旧、新 CLI 使用相同 Rust GNU 工具链及 release 优化配置。真实 SDL 运行时均检测到 **0 个物理手柄**，无会话、无虚拟驱动。每组 3 次、每次运行 12 秒；CPU 时间包含进程初始化及退出，数值按一个逻辑核心折算，本机共 20 个逻辑处理器。最终 CLI 与桌面一起构建，依赖功能组合可能不同；这项 CPU 比较是观察数据，不能把差异全部归因于源码优化。

| 版本 | 平均 CPU 时间/次 | 平均单核 CPU 占比 |
|---|---:|---:|
| 优化前完整原生引擎 | 156.25 ms | 1.2683% |
| 首轮优化引擎 | 151.04 ms | 1.2375% |
| 最终优化引擎 | 125.00 ms | 1.0246% |

首轮几乎没有差异，最终三次平均值较低，但最终各次在 0.77%～1.28% 之间波动，与旧版范围重叠。包含启动开销、依赖构建组合及 Windows CPU 计时粒度影响，**没有足够证据宣称 CPU 或内存有稳定的明显下降**。尤其不能据空设备结果推断连接实体手柄后的 CPU、发热或续航。此轮已证明的主要收益是移除发送等待、避免旧状态积压。

原始数据：[优化前](evidence/performance/baseline-engine-summary.json)、[首轮优化后](evidence/performance/optimized-engine-summary-initial.json)、[最终优化后](evidence/performance/optimized-engine-summary.json)。最终被测 CLI SHA256 为 `afcc8ed587dfdfeb9e542726306e50bace58ee2f6a2c77900bef9daf3a885014`；首轮为 `8455f0453480f22d0499af6c4b6bda6eeb582c6a6a898c390f5d7a8d9561bc61`；旧版为 `6664220ce3ecd8870262d48e06d9ccd9f1f1ce0e5f8b4cbf89ec78036c89cb58`。完整桌面程序的 UI/WebView CPU 没有包含在该 CLI 测量中。

## 重复测量

```powershell
$env:HORIPLAY_SDL3 = (Resolve-Path .\runtime\SDL3.dll).Path
cargo build --release --locked -p horiplay-cli
.\scripts\measure-input.ps1 -Executable .\target\release\horiplay.exe -OutputDirectory .\artifacts\performance -Samples 600 -Runs 3
.\scripts\measure-engine.ps1 -Executable .\target\release\horiplay.exe -Label optimized -OutputDirectory .\artifacts\performance -Seconds 12 -Runs 3
```

本机 GNU 构建使用 `CARGO_TARGET_DIR=D:\CodeX\Builds\HoriPlay`；命令中的路径应跟随各自环境。对照历史版本的 CPU 时，先在独立目录构建旧提交并保留其二进制，避免覆盖当前代码。测量时不要同时运行编译器或其他性能基准。

SDL 线程和枚举语义依据官方文档：[SDL_UpdateGamepads](https://wiki.libsdl.org/SDL3/SDL_UpdateGamepads)、[SDL_GetGamepads](https://wiki.libsdl.org/SDL3/SDL_GetGamepads)。单消费者通知按 [Tokio Notify 官方说明](https://docs.rs/tokio/latest/tokio/sync/struct.Notify.html) 使用，包含发布发生在等待之前的回归检查。
