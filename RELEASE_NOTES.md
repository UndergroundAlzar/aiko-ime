# Aiko IME Release Notes / 发布说明

## v1.6.0 - 可靠性与识别质量 / Reliability And Recognition Quality

### 中文

- 离线识别真正接入主流程：`asr.backend = "offline"` 现在会构建 sherpa-onnx provider 并走独立 PCM 音频通路；当 feature 构建、运行库 DLL 或模型缺失时，会弹窗说明原因并自动回退在线识别，不再静默忽略该配置。
- 设置中心选择的麦克风现已真正生效：录音按 `audio.input_device` 解析设备，找不到时回退系统默认并记录日志；录音中默认设备变化仍会自动重启采集。
- 重采样从最近邻取样改为整数倍均值抽取 + 线性插值，48kHz/44.1kHz 麦克风下送往 ASR 的音频不再有混叠失真；Opus 编码改用 VOIP 模式并固定 24kbps，更适合语音识别。
- 移除把整帧音频清零的硬噪声门（电平条显示阈值保留），安静的句首和句尾不再被截断。
- 网络鲁棒性：设备注册、取 token 和 AI 润色共享一个带连接/请求超时的 HTTP client，网络异常不会让启动或停止永久卡住；WebSocket 读取错误与会话中途断开现在会作为错误上报，而不是悄悄结束。
- 音频发送改为一帧 lookahead，把真实的最后一帧标记为 `Last`，不再向服务器追加伪造的全零帧。
- 确认停止和取消撤销的语义分离：确认会停止采集并等待后端最终结果；取消会中断在线/离线结果循环并撤回本次已输入文本。等待最终结果带超时保护，避免 UI 长时间卡住。
- 音频采集增加 generation token，旧采集线程无法在离线失败回退在线、快速重启或接收端关闭后继续向新会话送帧。
- 退出可靠性：托盘菜单和悬浮条的退出现在向主线程投递 `WM_QUIT`，修复点击退出后还需要再动一下鼠标程序才真正退出的问题。
- 修复配置保存竞态：拖动位置、尺寸、桌宠可见性等更新改为"读最新配置-改-存"，不再用启动时的旧配置副本覆盖设置中心刚保存的更改；`config.toml` 写入改为临时文件 + 重命名的原子操作，崩溃不会再留下半截配置。
- 桌宠状态修复：会话自然结束（服务器断开、目标窗口丢失）后桌宠不再卡在"倾听"状态，出错时会切换到错误表情；桌宠右键菜单的显示/隐藏现在会同步回托盘状态和配置文件。
- 双击 CapsLock 现在可以用作触发键（设置中心此前已有该选项，但底层钩子不支持）。
- 口述 emoji 关键词转换改为默认关闭的 `general.emoji_conversion_enabled` 开关：简单子串替换会误伤普通词语（如"我胜利了"会被改写）；同时非 IDE、非聊天应用不再强制剥离识别结果中的 emoji。
- 自定义词典替换改为按词条长度降序执行，重叠词条（如 "AI" 与 "AI 助手"）的行为现在可预测。
- UI 模式日志写入 exe 旁的 `aiko-ime.log.*` 按天滚动文件。release 版隐藏控制台，此前所有运行日志都会丢失，问题无法排查。
- 多显示器：桌宠恢复保存位置时会被钳制到当前虚拟桌面内，拔掉显示器后桌宠不会再出现在不可见区域。
- 清理：删除约 330 行死代码（旧的热键录制窗口）、移除未使用的 ring/base64 运行时依赖（ring 保留为资产校验测试依赖）、根目录图标处理脚本移至 `tools/icons/`、音频与控制器内的 `println!` 调试输出统一改为 `tracing`。

### English

- Offline recognition is now genuinely wired into the dictation flow. With `asr.backend = "offline"`, the controller builds the sherpa-onnx provider and feeds it raw PCM. When the feature build, runtime DLL, or model package is missing, the app explains why and falls back to online recognition instead of silently ignoring the setting.
- The microphone chosen in the settings center is now honored. Capture resolves `audio.input_device`, falls back to the system default with a logged warning when missing, and still restarts automatically when the default device changes mid-recording.
- Resampling switched from nearest-neighbor picking to integer-ratio averaging plus linear interpolation, removing aliasing on 48kHz/44.1kHz microphones. Opus encoding now uses VOIP mode at a fixed 24kbps, tuned for speech.
- Removed the hard noise gate that zeroed whole frames (the level-meter threshold remains); quiet onsets are no longer chopped.
- Network robustness: device registration, token fetch, and AI post-processing share one HTTP client with connect/request timeouts so a dead network can no longer hang startup or stop. WebSocket read errors and mid-session disconnects surface as errors instead of ending silently.
- Audio sending uses a one-frame lookahead so the genuine final frame is flagged `Last`; the fabricated all-zero trailing frame is gone.
- Confirm-stop and cancel-discard now have separate semantics: confirm stops capture and waits for backend finalization, while cancel interrupts the online/offline result loop and removes text typed in the current session. Finalization waits are timeout-protected so the UI does not hang indefinitely.
- Audio capture now carries a generation token so stale capture threads cannot feed frames into a new session after offline fallback, quick restart, or receiver shutdown.
- Quit reliability: tray and floating-control exits now post `WM_QUIT` to the main thread, fixing the bug where the app only exited after another stray mouse event.
- Fixed config save races: position, size, and pet-visibility updates reload-modify-save the latest file instead of overwriting newer settings with a stale startup copy, and `config.toml` is written atomically via temp-file + rename.
- Desktop pet fixes: the pet no longer sticks in "listening" when a session ends on its own (server hangup, lost target window) and shows the error face when that happens; hide/show from the pet's own context menu now syncs back to the tray flag and saved config.
- Double-tap CapsLock now works as a trigger key (the settings UI already offered it; the low-level hook did not support it).
- Spoken emoji-keyword conversion moved behind the off-by-default `general.emoji_conversion_enabled` switch, because plain substring replacement also rewrites ordinary words. Non-IDE, non-messenger apps no longer get emoji stripped from recognized text.
- Custom vocabulary replacements now run longest-term-first, making overlapping entries (such as "AI" and "AI 助手") deterministic.
- UI-mode logs are written to rolling `aiko-ime.log.*` files next to the executable. Release builds hide the console, so previously all runtime logs were lost.
- Multi-monitor: the pet's restored position is clamped into the current virtual desktop so it can no longer reappear on a disconnected monitor.
- Cleanup: removed ~330 lines of dead hotkey-recorder code, dropped unused ring/base64 runtime dependencies (ring stays as a test dependency for asset checksums), moved root icon scripts into `tools/icons/`, and converted stray `println!` debugging in audio/controller code to `tracing`.

## Unreleased - Cubism Contract And Fallback / Cubism 契约与回退

### 中文

- 增加真正 Cubism 模型的资产契约测试：若 `assets/live2d/aiko/runtime` 被标记为 `ready`，必须同时提供有效的 `.model3.json`、`.moc3`、纹理以及描述符引用的 motion、expression、physics 等文件。
- 新增 feature-gated 的 Cubism 运行时基础，覆盖 Core 动态加载、模型发现/校验和命令解析；桌宠窗口的最终 Native Framework 渲染适配器尚未完成。
- CI 会在清除常见 SDK 环境变量后验证 `cubism-live2d` feature 构建，防止开发机或 CI 因缺少专有 SDK 而无法构建 PNG fallback。
- 增加缺模型回退测试。配置优先选择 `cubism` 但本地没有模型时，桌宠会使用 PNG 渲染。
- portable 打包始终携带 PNG fallback、Live2D 资产 manifest 和离线建模规范；仅当 runtime 阶段为 `ready` 且通过严格检查时，才把 `assets/live2d/aiko/runtime` 映射到 `models/live2d`，并逐文件校验 SHA-256。
- 当前 v1.6.0 发布包没有捆绑可直接发布的真正 Cubism 模型。已有桌宠仍是 PNG 状态精灵驱动的轻量 Live2D 风格动画，不把未完成模型描述为已完成。
- 新增 Aiko 正面建模母版、12 表情口型表、头部角度表、动作表、五皮肤目录和多张展示插画。
- 新增 CAG Operator 固定皮肤方案：四筒 GPNVG、全指手套、胸前枪带与枪口朝下的步枪、`AIKO / B RH-` 胸贴，并明确不使用麦克风贴章或背景麦克风标志。
- CAG Operator 使用独立 Cubism 模型规范和装备增量分层表，当前诚实标记为 `concept_only`。

### English

- Added a genuine Cubism model asset contract. When `assets/live2d/aiko/runtime` is marked `ready`, its `.model3.json`, `.moc3`, textures, and every referenced motion, expression, physics, and related file must be present and valid.
- Added a feature-gated Cubism runtime foundation for Core dynamic loading, model discovery/validation, and command resolution. The final Native Framework renderer adapter for the desktop-pet window is not complete.
- CI clears common SDK environment variables before checking the `cubism-live2d` feature build so machines without the proprietary SDK can still build the PNG fallback.
- Added missing-model fallback coverage. When configuration prefers `cubism` but no local model is available, the desktop pet uses PNG rendering.
- Portable packages always contain the PNG fallback, Live2D asset manifest, and offline modeling guide. They map `assets/live2d/aiko/runtime` to `models/live2d` only when the runtime stage is `ready` and passes strict validation, with per-file SHA-256 checks.
- The current v1.6.0 package does not bundle a release-ready genuine Cubism model. Its desktop pet remains lightweight Live2D-style animation driven by PNG state sprites; unfinished model work is not presented as complete.
- Added a front-facing Aiko modeling master, a 12-expression mouth sheet, head-angle and motion sheets, a five-skin catalog, and multiple key illustrations.
- Added a fixed CAG Operator skin concept with four-tube GPNVG, full-finger gloves, a chest-retained muzzle-down rifle, `AIKO / B RH-` patches, and no microphone patch or background microphone symbol.
- Added a bilingual fixed-skin selector for Default, Pajama, Rain, Winter, Celebration, and CAG Operator Aiko variants.
- Added independent front-facing modeling masters and transparent cutouts for the Pajama, Rain, Winter, and Celebration skins.
- The CAG Operator uses a separate Cubism model specification and tactical layer addendum and is honestly marked `concept_only`.

## v1.5.0 - Release Quality Gate / 发布质量门禁

### 中文

- GitHub Actions 现在在 Windows 上执行完整发布门禁：`cargo fmt`、`cargo clippy`、`cargo test`、静态链接 Release 构建、portable 打包校验和离线 smoke test。
- 桌宠升级为轻量 Live2D 风格状态动画，支持待机、倾听、处理中、成功、错误、困倦和被摸头开心等状态。
- 桌宠事件已接入语音控制器：点击可开始/停止语音输入，拖动位置和尺寸会保存到配置文件。
- 新增 6 张 Aiko 状态素材和 README 预览图，便携包会携带完整资源树。
- CI 将 release build 和 portable packaging 拆成独立步骤，打包阶段使用 `build-portable.ps1 -SkipBuild` 复用已经验证过的二进制。
- 新增 portable 包布局校验：目录和 ZIP 都必须包含 `aiko-ime.exe`、`config.toml`、`README.md`、`RELEASE_NOTES.md`、`LICENSE`、`VERSION.txt` 和完整 `assets/` 目录。
- portable 校验会检查 EXE 的 PE/MZ 头、版本号、README 图片引用以及所有嵌套资源与源码资源的 SHA-256 一致性。
- 新增离线 Windows smoke test，覆盖 release EXE、单实例保护标记、热键状态机标记、Win32 窗口类和 portable 资源树，不访问豆包服务、不录音、不向当前窗口输入文字。
- 新增仓库契约测试，防止 CI 步骤、portable 布局、双语发布文档、配置模板和图片资源契约被意外破坏。

### English

- GitHub Actions now runs the complete Windows release gate: `cargo fmt`, `cargo clippy`, `cargo test`, a statically linked release build, portable package validation, and an offline smoke test.
- The desktop pet now uses lightweight Live2D-style state animation for idle, listening, processing, success, error, sleepy, and petted states.
- Desktop pet events are connected to the voice controller: clicks can start/stop dictation, and dragged position plus size are saved to the config file.
- Added six Aiko state sprites and a README preview image. Portable packages include the complete asset tree.
- CI separates the release build from portable packaging. The packaging step uses `build-portable.ps1 -SkipBuild` to reuse the binary that was already built and checked.
- Added portable layout validation for both the unpacked directory and ZIP archive. They must include `aiko-ime.exe`, `config.toml`, `README.md`, `RELEASE_NOTES.md`, `LICENSE`, `VERSION.txt`, and the complete `assets/` directory.
- Portable validation checks the EXE PE/MZ header, version file, README image references, and SHA-256 parity for every nested asset copied from the source tree.
- Added an offline Windows smoke test for the release EXE, single-instance marker, hotkey state-machine markers, Win32 window classes, and packaged resources without contacting Doubao, recording audio, or typing into the active window.
- Added repository contract tests to keep CI steps, portable layout, bilingual release documentation, config templates, and image assets from drifting.

## v1.4.0 - Offline ASR Foundation / 离线识别基础

### 中文

- 新增在线/离线 ASR provider 契约，统一会话开始、局部结果、最终结果、错误、完成和取消事件。
- 新增 Doubao provider 包装层，为后续在线/离线后端切换提供一致接口。
- 新增 `sherpa-onnx` feature-gated 离线识别基础设施，支持运行时动态加载 Windows x64 sherpa-onnx C API。
- 新增 sherpa-onnx 模型发现和 `aiko-sherpa-model.json` 清单校验，限制模型包路径不能越过模型目录。
- 新增运行库探测和版本门禁，支持显式 DLL 路径、环境变量、模型目录、便携运行库目录和 `%LOCALAPPDATA%` 运行库目录。
- 配置文件新增 `[asr]` 后端选择字段：默认仍为在线 Doubao，离线模式需要额外 feature 构建、运行库 DLL 和模型包。

### English

- Added a shared online/offline ASR provider contract with unified session-started, partial-result, final-result, error, finished, and cancelled events.
- Added a Doubao provider wrapper so future online/offline backend selection can use one interface.
- Added feature-gated `sherpa-onnx` offline recognition infrastructure with Windows x64 dynamic loading for the sherpa-onnx C API.
- Added sherpa-onnx model discovery and `aiko-sherpa-model.json` manifest validation, including path containment checks for model packages.
- Added runtime probing and version gating for explicit DLL paths, environment variables, model directories, portable runtime directories, and `%LOCALAPPDATA%` runtime directories.
- Added `[asr]` backend selection fields to the config. The default remains online Doubao; offline mode requires an additional feature build, runtime DLLs, and a model package.

## v1.3.0 - Settings And Local UX / 设置中心与本地体验

### 中文

- 新增原生 Windows 设置中心，可从系统托盘打开。
- 设置中心覆盖通用设置、热键、悬浮控件、桌宠、ASR、AI 后处理、翻译、自定义词典和本地历史记录开关。
- 新增麦克风测试入口，便于在不启动正式听写会话的情况下检查输入设备。
- 配置模板补充开机自启动、语言、历史文件、麦克风、在线/离线 ASR、VAD 和桌宠尺寸字段。
- 新增单实例保护：重复启动会提示使用已有实例，避免多个托盘程序和录音控制器同时运行。
- 桌宠配置和托盘开关会持久化，重启后保留用户选择。

### English

- Added a native Windows settings center that opens from the system tray.
- The settings center covers general preferences, hotkeys, floating controls, the desktop pet, ASR, AI post-processing, translation, custom vocabulary, and local history logging.
- Added a microphone test entry point so users can check input devices without starting a real dictation session.
- Expanded the config template with startup, language, history file, microphone, online/offline ASR, VAD, and desktop pet size fields.
- Added a single-instance guard. Duplicate launches now direct users to the existing instance instead of starting multiple tray apps and recording controllers.
- Desktop pet visibility and tray toggles are persisted so user choices survive restarts.

## v1.2.3 - Aiko Visuals And Stability / Aiko 视觉与稳定性

### 中文

- 新增 Aiko 视觉素材：应用图标、托盘图标、README 展示图和桌宠素材。
- 新增桌宠窗口：启动后可显示小 Aiko，并可从托盘菜单的 `显示/隐藏桌宠` 开关控制。
- 新增 `[desktop_pet]` 配置项：支持设置默认显示状态、初始位置和尺寸。
- 便携包和 GitHub Release 会包含 README 展示图，保证离线查看文档时图片也能显示。
- 应用程序 exe 已嵌入 Aiko 图标，Windows 文件管理器和快捷方式中会显示新图标。
- 修复部分网络环境下 ASR WebSocket 卡在不可用 IPv6 地址的问题。
- 修复旧缓存凭据导致 ASR 后端 `service discovery failure` 的问题。
- 修复录音过程中插入文字后，双击 `Ctrl` 无法再次停止录音的问题。
- CLI 的 ASR 测试现在会做真实 WebSocket 握手，而不是只打印本地凭据。
- 修复录音悬浮控制条透明区域被 DWM 背景填成白/灰色矩形的问题。
- 修复录音悬浮控制条首次弹出时可能短暂闪现白框的问题：现在会先绘制透明图层，再显示窗口。
- 修复豆包匿名注册偶尔返回异常短设备 ID、随后触发 `service discovery failure` 的问题；程序现在会拒绝异常凭据并自动重试注册。

### English

- Added Aiko visual assets: app icon, tray icon, README showcase image, and desktop pet artwork.
- Added a desktop pet window: small Aiko can be shown on launch and toggled from the tray menu with `显示/隐藏桌宠`.
- Added `[desktop_pet]` configuration for default visibility, initial position, and size.
- Portable packages and GitHub Releases include the README showcase image so the documentation renders correctly offline.
- The Windows executable now embeds the Aiko app icon for File Explorer and shortcuts.
- Fixed ASR WebSocket connection stalls on networks where the endpoint exposes an unreachable IPv6 route.
- Fixed stale cached credentials causing ASR backend `service discovery failure`.
- Fixed double-tap `Ctrl` failing to stop recording after dictated text was inserted.
- CLI ASR diagnostics now perform a real WebSocket handshake instead of only printing local credentials.
- Fixed the floating recording control showing a white/gray rectangle behind its transparent area.
- Fixed a possible white-frame flash when the recording control first appears by rendering the layered surface before showing the window.
- Fixed occasional short device IDs from Doubao anonymous registration causing `service discovery failure`; invalid credentials are now rejected and registration is retried automatically.

## Release Validation / 发布验证

每个 Pull Request、主分支提交和版本标签都会在 Windows 上执行以下质量门禁：

Every pull request, main-branch commit, and version tag runs these Windows quality gates:

- `cargo fmt --all -- --check`
- `cargo clippy --locked --all-targets --all-features`
- `cargo test --locked --all-targets`
- 静态链接的 Windows Release 构建 / Statically linked Windows release build
- 便携目录和 ZIP 的文件、版本、PE 头、README 图片引用及完整嵌套资源校验
  Portable directory and ZIP validation for files, version, PE header, README image references, and the complete nested asset tree
- 无麦克风、无豆包依赖的 Windows smoke test / Offline Windows smoke test without microphone or Doubao
- Cubism feature 缺 SDK 构建、缺模型 PNG 回退与真实模型引用链校验
  Cubism feature build without the SDK, missing-model PNG fallback, and genuine model reference validation

GitHub Release 附件为经过验证的便携 ZIP 和对应的 SHA-256 文件。便携包中包含
`aiko-ime.exe`、`config.toml`、`README.md`、`RELEASE_NOTES.md`、`LICENSE`、
`VERSION.txt`、`docs/LIVE2D_MODELING.md` 和完整的 `assets/` 目录。

GitHub Release assets are the validated portable ZIP and its SHA-256 file. The archive includes
`aiko-ime.exe`, `config.toml`, `README.md`, `RELEASE_NOTES.md`, `LICENSE`,
`VERSION.txt`, `docs/LIVE2D_MODELING.md`, and the complete `assets/` directory.

`.omc/`、`target/`、本地 `dist/` 工作区和源码树 `config.toml` 是本地构建或运行状态，
不会作为 GitHub Release 附件发布。

`.omc/`, `target/`, the local `dist/` workspace, and the source-tree `config.toml` are local
build or runtime state and are not published as GitHub Release assets.

只有 `assets/live2d/aiko/runtime` 标记为 `ready` 并通过严格检查时，portable 包才额外包含 `models/live2d`。没有模型时，PNG fallback 是预期且受支持的发布形态。

The portable archive additionally includes `models/live2d` only when `assets/live2d/aiko/runtime` is marked `ready` and passes strict validation. Without a model, PNG fallback is the expected and supported release form.
