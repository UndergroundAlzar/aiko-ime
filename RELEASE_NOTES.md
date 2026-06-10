# Aiko IME v1.2.3 Release Notes / 发布说明

## 中文

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

## English

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
