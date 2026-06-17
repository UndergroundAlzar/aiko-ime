# Aiko Live2D 模型资产工程规范

## 1. 结论与当前交付状态

本次交付建立了 Aiko Live2D 的目录、命名、参数、触摸区域、动作、表情、物理和运行时文件契约。
仓库当前只有规范，没有真实 Live2D 模型：

| 阶段 | 目录 | 当前状态 | 可否被应用加载 |
|---|---|---:|---:|
| source | `assets/live2d/aiko/source` | `spec_only` | 否 |
| ready | `assets/live2d/aiko/ready` | `missing` | 否 |
| runtime | `assets/live2d/aiko/runtime` | `missing` | 否 |

没有创建或伪造 `.moc3`、`.cmo3`、PSD 或分层 PNG。现有 `assets/mascot/**` 是桌宠状态图，
只能作为造型参考，不能声明为 Live2D 分层源文件。

## 2. 三阶段职责

### source：美术源与输入契约

保存 `aiko_master.psd`、同画布透明分层 PNG，以及五张机器可读清单：

- `layers.csv`：PSD 分组、图层 ID、PNG 文件名、裁剪关系和必需性。
- `parameters.csv`：参数 ID、范围、驱动方和用途。
- `hit-areas.csv`：触摸区域、ArtMesh ID、交互和优先级。
- `motions.csv`：动作组、文件、淡入淡出、循环和状态映射。
- `expressions.csv`：表情名、文件、淡入淡出和用途。

source 资产不可直接随应用当作 Cubism 模型加载。

### ready：Cubism Editor 可编辑工程

至少包含真实 `aiko.cmo3`。模型师应在此阶段完成：

1. PSD 重导入无丢层，所有必需图层均已成为 ArtMesh。
2. 参数 ID 和范围与 `parameters.csv` 一致。
3. 眼睛、口型、头身转动、头发与领结物理可用。
4. `D_Hit_Head`、`D_Hit_Mic`、`D_Hit_Body` 三个触摸网格存在。
5. 七个桌宠状态动作和七个表情通过预览。
6. 纹理图集无越界、黑边、脏像素和不必要的大面积空白。

ready 工程仍不可直接作为 SDK 运行包。

### runtime：Cubism SDK 导出包

唯一入口是 `runtime/aiko.model3.json`。运行包至少包含：

```text
aiko.model3.json
aiko.moc3
aiko.physics3.json
aiko.cdi3.json                 # 建议提供
textures/texture_00.png
motions/*.motion3.json
expressions/*.exp3.json
```

`.moc3` 必须由 Cubism Editor 正式导出。验证脚本会检查 `MOC3` 文件头、引用闭包和 JSON
结构，但文件头检查不替代 Cubism SDK 的实际加载测试。

## 3. PSD 与 PNG 命名

完整清单见 `assets/live2d/aiko/source/layers.csv`。

统一规则：

- 交付 ID 和文件名使用小写 ASCII `snake_case`。
- PNG 固定在 `source/png/parts/`，文件名与 `layer_id` 一致。
- 左右以角色自身视角命名，后缀只用 `_l`、`_r`。
- PSD 分组可使用数字排序前缀；ArtMesh、参数和运行时 ID 不带排序前缀。
- 分层 PNG 保留统一画布，不裁透明边，不缩放，不改变原点。
- 眼白、虹膜、瞳孔、高光独立；嘴线、口腔、舌和牙独立。
- 前发、侧发、后发摆片独立；上臂、前臂和手独立。
- `hit_*` 是建模参考层，最终对应不可见触摸 ArtMesh，不作为可见绘制内容。

PSD 建议组：

```text
00_reference
05_shadow
10_head/
  11_hair_back
  12_face_base
  13_eyes
  14_brows
  15_mouth_nose
  16_hair_side
  17_hair_front
20_body/
  21_skin
  22_clothes
  23_ribbon
30_arms/
  31_arm_l
  32_arm_r
40_accessories/
  41_headset
  42_microphone
50_effects
90_hit_areas
```

## 4. 参数表

完整参数和范围见 `parameters.csv`。核心契约：

| 参数 | 范围 | 驱动 | 用途 |
|---|---:|---|---|
| `ParamAngleX/Y/Z` | `-30..30` | pointer/motion | 头部三轴 |
| `ParamBodyAngleX/Y/Z` | `-10..10` | pointer/motion | 身体三轴 |
| `ParamEyeLOpen/ROpen` | `0..1` | eye_blink | 左右眨眼 |
| `ParamEyeBallX/Y` | `-1..1` | pointer | 视线跟随 |
| `ParamMouthOpenY` | `0..1` | lip_sync | 音量口型 |
| `ParamMouthForm` | `-1..1` | expression | 嘴型情绪 |
| `ParamBreath` | `0..1` | auto | 呼吸 |
| `ParamHair*`、`ParamRibbon*` | `-1..1` | physics | 物理摆动 |
| `ParamAikoMicGlow` | `0..1` | state | 倾听状态 |
| `ParamAikoThinking` | `0..1` | state | 处理中状态 |
| `ParamAikoPet` | `0..1` | interaction | 摸头反馈 |
| `ParamAikoSleep` | `0..1` | state | 困倦状态 |

`model3.json` 的 `Groups` 必须声明：

- `EyeBlink`：`ParamEyeLOpen`、`ParamEyeROpen`
- `LipSync`：`ParamMouthOpenY`

## 5. 触摸区域

触摸优先级从高到低：

| Name | ArtMesh ID | 交互 | 优先级 |
|---|---|---|---:|
| `Head` | `D_Hit_Head` | 抚摸和主单击反馈 | 100 |
| `Mic` | `D_Hit_Mic` | 开始/停止语音输入 | 90 |
| `Body` | `D_Hit_Body` | 拖动桌宠窗口 | 10 |

区域应贴合实际轮廓，不得用覆盖整个画布的矩形。命中重叠时按表中优先级处理。
头部移动累计触发 `petted`，行为与现有桌宠状态机保持一致。

## 6. 动作与状态映射

完整动作表见 `motions.csv`。必需动作组：

| 动作组 | 运行方式 | 触发 | 结束后 |
|---|---|---|---|
| `idle` | 循环 | 待机 | `idle` |
| `listening` | 循环 | 正在收音 | `listening` |
| `processing` | 循环 | 正在处理 | `processing` |
| `success` | 单次 | 输入成功 | `idle` |
| `error` | 单次 | 输入失败 | `idle` |
| `petted` | 单次 | 摸头/主单击反馈 | 前一状态 |
| `sleepy` | 循环 | 待机 45 秒 | `sleepy` |

动作组名必须与现有 `PetState` 小写形式一致。`success`、`error` 和 `petted` 的保持时间由应用状态机
控制，动作本身不得用超长静止尾帧拖延切换。

## 7. 表情契约

必需表情为 `neutral`、`listening`、`processing`、`success`、`error`、`petted`、`sleepy`。

- 表情文件只调整情绪和状态参数，不写入持续头身位移。
- `neutral` 必须能覆盖清除前一状态残留。
- `listening` 驱动 `ParamAikoMicGlow`。
- `processing` 驱动 `ParamAikoThinking`。
- `petted` 驱动笑眼、脸红和 `ParamAikoPet`。
- `sleepy` 驱动闭眼和 `ParamAikoSleep`。
- `error` 可选驱动 `ParamAikoTear`，但不可把泪滴设为常驻。

## 8. 文件契约

项目级 JSON Schema 位于 `assets/live2d/contracts/`。

### `aiko.model3.json`

- `Version` 必须为 `3`。
- 必须引用真实 `.moc3`、至少一张纹理、physics、七组动作和七个表情。
- 所有路径必须是 runtime 内相对路径，禁止 `..`、盘符、UNC 和反斜杠逃逸。
- `HitAreas` 必须含 `Head`、`Mic`、`Body`。
- `Groups` 必须含 `EyeBlink` 和 `LipSync`。

### `*.motion3.json`

- `Version` 为 `3`，包含 `Meta` 和非空 `Curves`。
- `Meta.Duration > 0`、`Fps >= 24`，曲线/片段/点计数为正。
- `Curves[].Target` 只允许 `Model`、`Parameter`、`PartOpacity`。

### `*.exp3.json`

- `Type` 为 `Live2D Expression`。
- 参数项包含 `Id`、`Value`、`Blend`。
- `Blend` 只允许 `Add`、`Multiply`、`Overwrite`。

### `aiko.physics3.json`

- `Version` 为 `3`，物理设置非空。
- 每组包含输入、输出、粒子顶点和归一化设置。
- 至少覆盖前发、左右侧发、后发和左右领结。

## 9. 验证

当前仓库可运行契约检查：

```powershell
pwsh -NoProfile -File scripts/Test-Live2DAssets.ps1
```

真实资产进入相应阶段后使用严格门禁：

```powershell
pwsh -NoProfile -File scripts/Test-Live2DAssets.ps1 -RequireSource
pwsh -NoProfile -File scripts/Test-Live2DAssets.ps1 -RequireReady
pwsh -NoProfile -File scripts/Test-Live2DAssets.ps1 -RequireRuntime
pwsh -NoProfile -File scripts/Test-Live2DAssets.ps1 -RequireAll
```

默认检查会验证规范、CSV、Schema、命名和已存在资产，并明确报告缺失阶段；不会把缺模型误报为成功模型。
严格 runtime 检查通过后，还需在目标渲染器中验证加载、透明背景、纹理、眨眼、口型、物理、动作切换和触摸命中。

## 10. 模型师交付步骤

1. 按 `layers.csv` 制作 `aiko_master.psd` 和同画布透明 PNG。
2. 把 `source.status` 改为 `ready`，运行 `-RequireSource` 并修正全部问题。
3. 在 Cubism Editor 完成建模，保存真实 `ready/aiko.cmo3`。
4. 把 `ready.status` 改为 `ready`，运行 `-RequireReady` 并修正全部问题。
5. 从 Cubism Editor 导出 SDK 模型到 `runtime/`，不得手写 `.moc3`。
6. 补齐 model3 引用，把 `runtime.status` 改为 `ready`，运行 `-RequireRuntime`。
7. 在应用集成测试通过后，保留 `runtime.status = ready` 并记录导出版本。

## 11. 固定皮肤与 CAG Operator 模型

首批固定内置皮肤规划为：

1. 默认 Aiko
2. 居家睡衣
3. 雨天
4. 冬装
5. 庆祝
6. CAG Operator

皮肤概念图位于 `assets/aiko_art/skins/`。为避免许可和安全边界失控，应用不开放
任意 `.model3.json` 导入、模型市场或用户上传入口，只加载随应用固定发布的 Aiko 模型。

CAG Operator 使用独立模型 `aiko_cag`，增量分层清单位于
`assets/live2d/aiko_cag/source/layers-addon.csv`。必须满足：

- GPNVG 拆成底座和四个独立镜筒，四筒数量不可合并或省略。
- 高切盔、导轨、左右通讯耳机和 boom mic 分层；boom mic 只是装备，不作为
  Aiko IME 或其他软件的视觉标志。
- plate carrier、弹匣袋、无线电、通讯线、医疗包、姓名贴和血型贴分层。
- 姓名贴固定为 `AIKO`，血型贴固定为 `B RH-`。
- 左右手均为完整全指手套，不复用默认模型裸手。
- KAC KS-1 风格步枪通过左右枪带固定在胸前，枪口朝下；枪、枪带和装备投影独立。
- 枪带、通讯线、左右腰包使用低幅度物理；枪体只允许轻微惯性，不能像布料一样摆动。
- 所有动态动作保持手不持枪。点赞、挥手、摸头反馈等动作必须维持枪口朝下。

正面母版：

```text
assets/aiko_art/modeling/aiko_cag_master_source.png
```

该图片仍是平面概念母版。只有完成分层 PSD、Cubism Editor 建模并正式导出后，
`assets/live2d/aiko_cag/` 才能从 `concept_only` 进入 `ready` / `runtime`。
