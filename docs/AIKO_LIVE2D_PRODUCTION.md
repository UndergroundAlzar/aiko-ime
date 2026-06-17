# Aiko Live2D Production Workspace / Aiko Live2D 生产工作区

## 中文

### 1. 工具边界

本流程把现有母版、透明轮廓、表情、头部角度和动作图整理为可复现的模型师交付工作区。
它只做以下事情：

- 固定参考图路径、尺寸和 SHA-256。
- 复制参考图与 `layers.csv`、`parameters.csv`、`hit-areas.csv`、
  `motions.csv`、`expressions.csv`。
- 建立 source、ready、runtime 和 review 目录。
- 生成 `workspace-manifest.json` 并校验复制一致性。

它不会自动拆件、补绘、生成分层 PSD、创建 ArtMesh、绑定参数、制作物理或动作，也不会生成
或伪造 `.cmo3`、`.moc3`、`.model3.json`。PNG 母版和切图仅是视觉参考，不能改后缀冒充
PSD 或 Cubism 文件。

### 2. 准备工作区

从仓库根目录运行：

```powershell
pwsh -NoProfile -File scripts/Prepare-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff

pwsh -NoProfile -File scripts/Test-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff
```

准备脚本可以重复运行。它会刷新 `references/`、`contracts/` 和
`workspace-manifest.json`，不会删除或覆盖 `handoff/` 中模型师制作的文件。

```text
AikoLive2D-Handoff/
|-- workspace-manifest.json
|-- references/                 # 固定的平面参考图
|-- contracts/                  # 图层、参数、触摸、动作、表情契约
|-- handoff/
|   |-- source/
|   |   |-- aiko_master.psd     # 人工分层后交付
|   |   `-- png/parts/          # 与 PSD 同画布、同原点的透明分层 PNG
|   |-- ready/
|   |   `-- aiko.cmo3           # Cubism Editor 保存
|   `-- runtime/
|       `-- ...                 # Cubism Editor 正式 SDK 导出
`-- review/                     # 截图、录屏和人工验收记录
```

### 3. 必须人工完成的美术步骤

1. 以 `references/aiko_live2d_master_source.png` 为造型与配色基准，以透明轮廓、
   表情、角度和动作图交叉校对。
2. 按 `contracts/layers.csv` 人工拆分和补绘遮挡区域。眼睛、眉、嘴、前后发、四肢、
   耳机、麦克风与效果件不得只靠自动抠图。
3. 创建真实 `handoff/source/aiko_master.psd`。画布固定为 1024 x 1536、RGB、
   8 bit/通道、透明背景。
4. 每个 `required=true` 图层导出为 `handoff/source/png/parts/<layer_id>.png`；
   保留完整画布和统一原点，不自动裁透明边。
5. 在 Photoshop/Krita 等图层编辑器中检查组名、层级、左右命名、透明洞、接缝和极限姿势补绘。

结构校验：

```powershell
pwsh -NoProfile -File scripts/Test-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff -RequireLayeredSource
```

该命令只检查 PSD 文件头、画布和必需 PNG；它不能证明 PSD 真的正确分层，人工图层审查仍是必需项。

### 4. 必须在 Live2D Cubism Editor 完成

- 导入并逐层核对真实 PSD，创建 ArtMesh、Warp Deformer 和 Rotation Deformer。
- 按 `parameters.csv` 创建参数、范围、默认值与绑定，处理绘制顺序和裁剪关系。
- 制作头部/身体角度、眼睛、眉、口型、呼吸、手臂和状态参数。
- 按 `hit-areas.csv` 创建不可见触摸网格。
- 配置头发和领结物理，检查所有参数及物理极值下的穿插和补绘。
- 按 `expressions.csv` 与 `motions.csv` 制作并预览表情、待机和状态动作。
- 保存真实可编辑工程为 `handoff/ready/aiko.cmo3`。
- 使用 Cubism Editor 的 SDK 导出功能生成 `.moc3`、纹理、physics、motion、
  expression 和 `aiko.model3.json` 到 `handoff/runtime/`。
- 在目标渲染器中完成透明背景、纹理、眨眼、口型、物理、动作切换和触摸命中测试。

```powershell
pwsh -NoProfile -File scripts/Test-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff -RequireCubismProject

pwsh -NoProfile -File scripts/Test-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff -RequireRuntimeExport

pwsh -NoProfile -File scripts/Test-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff -RequireAll
```

校验器能拒绝空文件、明显占位文本、错误 PSD/MOC3 文件头、缺失 runtime 引用和错误纹理；
它不能证明 `.cmo3` 的建模质量，也不能替代 Cubism Editor 打开检查和目标运行时加载测试。

## English

### 1. Tool boundaries

This workflow turns the existing master, transparent cutout, expression, head-angle, and motion
sheets into a reproducible modeler handoff workspace. It pins paths, dimensions, and SHA-256
values; copies reference images and CSV contracts; creates source/ready/runtime/review directories;
and writes a `workspace-manifest.json`.

It does not separate artwork, redraw occluded regions, create a layered PSD, generate ArtMeshes,
bind parameters, rig physics or motions, or create/fake `.cmo3`, `.moc3`, or `.model3.json` files.
A flat PNG must never be renamed and presented as a PSD or Cubism deliverable.

### 2. Prepare and validate

Run from the repository root:

```powershell
pwsh -NoProfile -File scripts/Prepare-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff

pwsh -NoProfile -File scripts/Test-AikoLive2DSource.ps1 `
  -WorkspaceRoot C:\AikoLive2D-Handoff
```

The prepare command is repeatable. It refreshes generated references, contracts, and the workspace
manifest without deleting or overwriting modeler-owned files under `handoff/`.

### 3. Artist/modeler work

1. Use the master source for design and color, and cross-check the cutout, expression, angle, and
   motion sheets.
2. Separate and redraw parts manually according to `contracts/layers.csv`.
3. Deliver a real 1024 x 1536 RGB 8-bit layered `handoff/source/aiko_master.psd`.
4. Export every required layer as a full-canvas alpha PNG at
   `handoff/source/png/parts/<layer_id>.png`, preserving the common origin.
5. Review layer order, left/right naming, holes, seams, and redraw coverage in a layer editor.

`-RequireLayeredSource` checks the PSD envelope, canvas, and required PNG files. It does not prove
that the PSD is meaningfully layered; manual layer review is mandatory.

### 4. Work that must be done in Live2D Cubism Editor

- Import and inspect the real layered PSD; create ArtMeshes and deformers.
- Create and bind all contracted parameters, draw order, clipping, and hit areas.
- Rig head/body angles, eyes, brows, mouth, breathing, arms, state parameters, and physics.
- Author and preview contracted expressions and motions, including extreme-value inspection.
- Save the editable project as `handoff/ready/aiko.cmo3`.
- Use the official SDK export to produce `.moc3`, textures, physics, motions, expressions, and
  `aiko.model3.json` under `handoff/runtime/`.
- Load-test the export in the target renderer.

`-RequireCubismProject` and `-RequireRuntimeExport` provide structural gates only. Passing them is
not authenticity certification and does not replace opening the project in Cubism Editor or testing
the official export in the target runtime.
