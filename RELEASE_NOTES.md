# Aiko IME Release Notes / 发布说明

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

GitHub Release 附件为经过验证的便携 ZIP 和对应的 SHA-256 文件。便携包中包含
`aiko-ime.exe`、`config.toml`、`README.md`、`RELEASE_NOTES.md`、`LICENSE`、
`VERSION.txt` 和完整的 `assets/` 目录。

GitHub Release assets are the validated portable ZIP and its SHA-256 file. The archive includes
`aiko-ime.exe`, `config.toml`, `README.md`, `RELEASE_NOTES.md`, `LICENSE`,
`VERSION.txt`, and the complete `assets/` directory.
