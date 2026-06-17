# Live2D Next Steps / Live2D 后续步骤

更新日期 / Updated: 2026-06-11

## 总原则 / Governing Rule

**不要继续生图。** 美术参考已全部生成；后续工作的瓶颈是人工分层与 Cubism 建模、
官方 SDK 私有集成、WebView2 Composition Controller 和真实运行验证。

Do not generate more images. The remaining work is human modeling, licensed SDK integration,
Composition Controller hosting, and real runtime verification.

## Phase 0: 保持诚实的基线 / Preserve The Honest Baseline

- [ ] 保持默认构建可在没有 Cubism SDK、Core 和模型时编译运行。
- [ ] 保持 PNG 桌宠为可靠 fallback。
- [ ] 保持 `cubism-live2d` 与 `webview2-renderer` 为 opt-in feature。
- [ ] 不改 `runtime` 状态为 `ready`，直到真实导出和目标运行时测试通过。
- [ ] 不创建任何假 PSD、CMO3、MOC3 或 model3。

基线命令 / Baseline commands:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features
cargo test --locked --all-targets
pwsh -NoProfile -File scripts/Test-CagArtAssets.ps1
pwsh -NoProfile -File scripts/Test-Live2DAssets.ps1
pwsh -NoProfile -File scripts/Test-CubismBuildMatrix.ps1
```

## Phase 1: 人工分层源文件 / Human Layered Source

负责角色 / Owner: artist or Live2D modeler.

1. 准备默认 Aiko 工作区：

```powershell
pwsh -NoProfile -File scripts/Prepare-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff
```

2. 以工作区 `references/` 中现有图片为唯一视觉输入，不再生图。
3. 按 `contracts/layers.csv` 人工拆层和补绘遮挡，制作真实
   `handoff/source/aiko_master.psd`。
4. 默认模型画布固定为 1024x1536、RGB、8 bit/通道、透明背景。
5. 每个 required 图层导出为全画布透明
   `handoff/source/png/parts/<layer_id>.png`，保持统一原点。
6. 人工检查透明洞、发片、眼口分件、左右命名、接缝和极限角度补绘。

结构门禁 / Structural gate:

```powershell
pwsh -NoProfile -File scripts/Test-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff -RequireLayeredSource
```

该命令只检查文件结构，不证明 PSD 真正分层正确。
This gate does not certify meaningful layer quality.

## Phase 2: Cubism Editor 人工建模 / Human Cubism Editor Work

负责角色 / Owner: trained Cubism modeler using Live2D Cubism Editor.

- [ ] 导入真实 PSD，创建 ArtMesh。
- [ ] 建立 Warp Deformer 和 Rotation Deformer。
- [ ] 绑定 `parameters.csv` 中全部 required 参数和范围。
- [ ] 配置 draw order、clipping、EyeBlink、LipSync 和 hit areas。
- [ ] 制作头身角度、眼眉、口型、呼吸、状态参数。
- [ ] 配置头发、侧发、后发、领结等物理。
- [ ] 按 `expressions.csv` 制作 7 个必需表情。
- [ ] 按 `motions.csv` 制作 7 个桌宠状态动作。
- [ ] 检查所有参数/物理极值下的穿插、透明洞与遮挡补绘。
- [ ] 保存真实 `handoff/ready/aiko.cmo3`。

CMO3 结构门禁：

```powershell
pwsh -NoProfile -File scripts/Test-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff -RequireCubismProject
```

**禁止事项 / Forbidden:** 空 CMO3、改后缀文件、ZIP/JSON 冒充 CMO3、自动生成假工程。

## Phase 3: Editor 正式导出 / Official Editor Export

在 Cubism Editor 中使用 embedded/SDK export：

1. 编辑纹理图集，确认透明边、采样和目标尺寸。
2. 导出 `.moc3`、纹理和 `.model3.json`。
3. 同步导出 `.physics3.json`。
4. 从 Animation/Viewer 正式导出 `.motion3.json` 和 `.exp3.json`。
5. 建议导出 `.cdi3.json`，保留参数/部件名称映射。
6. 确保 model3 的 `FileReferences` 只使用 runtime 内相对路径。
7. 在 Cubism Viewer 中加载 `aiko.model3.json`，逐项播放动作、表情和物理。

目标目录：

```text
assets/live2d/aiko/runtime/
|-- aiko.model3.json
|-- aiko.moc3
|-- aiko.physics3.json
|-- aiko.cdi3.json
|-- textures/*.png
|-- motions/*.motion3.json
`-- expressions/*.exp3.json
```

导出门禁：

```powershell
pwsh -NoProfile -File scripts/Test-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff -RequireRuntimeExport

pwsh -NoProfile -File scripts/Test-Live2DAssets.ps1 -RequireRuntime
```

通过脚本不等于完成。还必须通过 Cubism Viewer 和应用目标渲染器加载。

Official Editor references:

- <https://docs.live2d.com/en/cubism-editor-manual/export-moc3-motion3-files/>
- <https://docs.live2d.com/en/cubism-editor-manual/export-model3-json/>
- <https://docs.live2d.com/en/cubism-editor-manual/file-type-and-extension/>

## Phase 4: 官方 Cubism Web SDK 私有适配 / Private Official Web SDK Adapter

负责代码 / Code owner: developer with a valid Live2D SDK setup.

需要的私有输入：

```text
assets/live2d/web_runtime/vendor/live2dcubismcore.min.js
assets/live2d/web_runtime/vendor/aiko-cubism-web-sdk.js
assets/live2d/<skin>/runtime/<real Editor export>
```

实现 `globalThis.AikoCubismWebSdk.createRuntime()`：

- [ ] 使用官方 `CubismModelSettingJson` 解析 model3。
- [ ] 使用官方 `CubismUserModel` / Core 加载真实 moc3。
- [ ] 绑定纹理并初始化官方 Cubism WebGL renderer。
- [ ] 实现 render/update loop、delta time、resize 和 context restore。
- [ ] 实现参数写入与权重混合。
- [ ] 实现 motion priority、motion finished callback 和 stop-all。
- [ ] 实现 expression set/reset。
- [ ] 实现 eye blink、breath、physics、look-at 和 lip-sync 顺序。
- [ ] 实现 model3 hit-area hit test。
- [ ] 实现 model release 与 WebGL/Cubism resource cleanup。
- [ ] 将日志和错误通过公开协议回传，不静默切换到假渲染。

公开接口契约见 `assets/live2d/web_runtime/README.md` 和
`assets/live2d/web_runtime/runtime.js`。

不要把 Live2D Core 或 Framework 的许可内容复制进公共仓库，除非许可审查明确允许。

## Phase 5: WebView2 Composition Controller / WebView2 视觉托管

### 5.1 当前代码不能被误报为完成

`src/ui/live2d_web/window.rs` 当前使用
`ICoreWebView2Environment::CreateCoreWebView2Controller`，创建真实子 HWND。
它已覆盖透明背景设置、virtual host mapping、导航、ExecuteScript、web message、
resize、close 和 Windows smoke test，但不是 Composition Controller。

### 5.2 建议实现顺序

1. 新建独立的 Composition host/smoke 路径，先不要直接破坏 PNG 桌宠。
2. 为 `windows`/WebView2 依赖启用实际需要的 DirectComposition、D3D、DXGI 接口。
3. 将 environment cast/query 为 `ICoreWebView2Environment3`。
4. 使用 `CreateCoreWebView2CompositionController(parent_hwnd, handler)`。
5. 保存：
   - `ICoreWebView2CompositionController`
   - 基础 `ICoreWebView2Controller`
   - `ICoreWebView2`
   - DComp device、HWND target、root visual、WebView visual
6. 设置 `RootVisualTarget` 后 commit DComp device。
7. 使用基础 controller 设置 bounds、visibility、rasterization scale 和 close。
8. 保持 `SetDefaultBackgroundColor` alpha 为 0，并验证 canvas alpha。
9. 处理 resize、DPI、monitor change、device loss 和 runtime process failure。
10. 卸载时先断开 visual，再移除 event token、close controller、释放 DComp 资源。

### 5.3 输入转发

Composition Controller 不自动拥有子 HWND 输入。桌宠窗口需要：

- [ ] 将 `WM_MOUSEMOVE`、button down/up、wheel/hwheel 转换为 client point。
- [ ] 调用 `SendMouseInput`，正确携带 virtual-key flags 和 wheel delta。
- [ ] 需要触控/笔时使用 `SendPointerInput`。
- [ ] 处理 `CursorChanged` / `SystemCursorId`。
- [ ] 处理 capture、leave、focus 和 no-activate 窗口行为。
- [ ] 保留现有桌宠拖动逻辑；Body hit area 可进入 native drag，Head/Mic 进入交互。
- [ ] interactions disabled 时保持 click-through 语义。

### 5.4 与现有 layered PNG 窗口的迁移

当前 `AikoDesktopPetWindow` 使用：

```text
WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
UpdateLayeredWindow
```

不要在没有视觉验证的情况下把 DComp 路线直接塞进这条绘制路径。建议：

1. 先用专用普通 Win32 测试窗口证明 composition visual 可见。
2. 再验证透明 top-level pet window、alpha、阴影、置顶和 no-activate。
3. 再验证鼠标穿透、拖动、hit areas、DPI 和多显示器。
4. 最后让 renderer selection 在 `cubism` 成功时使用 composition host，否则保留 PNG。

Official Microsoft references:

- <https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2compositioncontroller>
- <https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2environment3>
- <https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/overview-features-apis>

## Phase 6: 桌宠状态桥接 / Desktop Pet Integration

- [ ] 在桌宠 UI 线程创建/销毁 composition host。
- [ ] 使用安全 virtual host 映射 `assets/live2d`，导航到
  `web_runtime/index.html`。
- [ ] 等待 `RuntimeReady` 后才发送 `LoadBuiltinModel`。
- [ ] 使用 `Live2dController::initial_host_commands()` 初始化模型。
- [ ] 将 `PetState` 变化发送为 motion、expression 和参数命令。
- [ ] 将音频幅度限制到 `0..1` 后发送 lip-sync。
- [ ] 将指针坐标归一化到 `-1..1` 后发送 look-at/hit-test。
- [ ] 将 `RuntimeEvent::HitArea` 映射为 Head/Pet、Mic/ToggleListening、Body/Drag。
- [ ] 处理 model load、motion finished、recoverable error 和 fatal error。
- [ ] SDK、model、WebView2 Runtime 或 renderer 初始化失败时记录原因并退回 PNG。
- [ ] 不开放任意 model URL、用户导入或模型市场。

## Phase 7: 测试矩阵 / Test Matrix

### 无专有 SDK的公共仓库 / Public no-SDK matrix

```powershell
cargo check --locked --all-targets --no-default-features
cargo test --locked --all-targets --no-default-features --features cubism-live2d
cargo test --locked --all-targets --features webview2-renderer
pwsh -NoProfile -File scripts/Test-CubismBuildMatrix.ps1
pwsh -NoProfile -File scripts/Test-Live2DAssets.ps1
```

预期：编译和契约测试通过；缺 Core/adapter/model 明确报错；PNG fallback 可用。

### 有许可私有输入 / Licensed private matrix

```powershell
cargo test --locked --all-targets --all-features
cargo test --locked --test live2d_web_host --features webview2-renderer
pwsh -NoProfile -File scripts/Test-Live2DAssets.ps1 -RequireAll
pwsh -NoProfile -File scripts/build-portable.ps1
```

人工 smoke checklist：

- [ ] 模型真实可见，背景完全透明，无黑框。
- [ ] idle 60 秒稳定，无持续错误或资源增长。
- [ ] listening/processing/success/error/petted/sleepy 全部切换正确。
- [ ] motion finish、expression reset 和 state restore 正确。
- [ ] 眨眼、呼吸、物理、look-at、lip-sync 同时工作。
- [ ] Head、Mic、Body hit area 命中正确。
- [ ] 拖动、点击、hover、click-through 和 no-activate 正确。
- [ ] 96/125/150/200% DPI 与多显示器 resize 正确。
- [ ] WebView2 process crash/device loss 后可恢复或回退 PNG。
- [ ] 关闭/重开桌宠不泄漏 WebView2、DComp、WebGL 或 event handler。

## Phase 8: 打包与许可 / Packaging And Licensing

- [ ] 代码继续保留 MIT `LICENSE`。
- [ ] 单独记录 Live2D Editor、SDK、Core、Framework、模型与 WebView2 Runtime 的许可来源。
- [ ] 阅读 SDK 下载包内 `LICENSE.md` / `NOTICE.md` 和 Live2D 当前发布条款。
- [ ] 记录 Cubism SDK/Core/Editor 导出版本和模型导出版本。
- [ ] 不把私有 SDK 输入意外加入公共 source archive。
- [ ] 若使用 WebView2 Evergreen，安装/缺失检测遵循 Microsoft 当前文档。
- [ ] 若使用 Fixed Version，随包版本、更新策略、体积和许可 notices 一并审查。
- [ ] 对最终 portable 内容做逐文件清单和第三方 notice 审核。

Microsoft recommends Evergreen for most applications:
<https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version>

Live2D Core and SDK license information must be taken from the official distribution:
<https://docs.live2d.com/en/cubism-sdk-manual/cubism-core/>

## 完成定义 / Definition Of Done

只有同时满足以下条件，才可说 “真实 Live2D 已完成”：

1. 真实分层 PSD 经人工审查。
2. 真实 CMO3 可在 Cubism Editor 打开继续编辑。
3. 真实 Editor runtime export 通过严格资产门禁。
4. 官方 SDK/Core 与私有 adapter 在许可范围内集成。
5. Composition Controller 实际显示透明模型并正确转发输入。
6. 桌宠状态、动作、表情、物理、口型和 hit areas 端到端通过。
7. 缺依赖时 PNG fallback 仍可靠。
8. portable 构建、测试、许可清单和第三方 notices 通过审查。

Before all eight conditions pass, describe the work as “flat art complete, Cubism integration in
progress,” not “Live2D complete.”
