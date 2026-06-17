# Aiko CAG Operator Live2D Model

这是 Aiko CAG Operator 固定内置皮肤的可建模资产契约。当前阶段为
`concept_only`：已有平面母版和机器可读清单，但没有分层 PSD、Cubism Editor
工程或 SDK 导出模型。

## 当前可验证内容

- `source/reference.json` 固定母版和透明预览的路径、尺寸、PNG 色彩类型与 SHA-256。
- `source/layers.csv` 是完整模型分层契约。
- `source/layers-addon.csv` 是 CAG 战术装备子集，所有行必须能在完整分层表中找到。
- `source/parameters.csv`、`motions.csv`、`expressions.csv` 定义建模和动画交付面。
- `source/occlusion-redraw.csv` 定义被头盔、胸挂、枪械和手套遮挡区域的补绘要求。
- `docs/CAG_MODELING_CHECKLIST.md` 是模型师交付与验收清单。

核心造型不可变：四筒 GPNVG、左右全指手套、KAC KS-1 胸前枪带悬挂且枪口朝下、
`AIKO` 与 `B RH-` 胸贴。通讯耳机的实体 boom mic 可以存在，但不得出现麦克风
图标、麦克风贴章或软件品牌标志。

## 阶段边界

当前目录不得出现 `.cmo3`、`.moc3` 或 `.model3.json`。完成真实分层 PSD 后才可把
`source.status` 改为 `ready`；只有 Cubism Editor 保存的真实 `.cmo3` 和正式 SDK
导出包通过严格检查后，才能推进 `ready` 与 `runtime` 阶段。禁止空文件、改后缀、
手写 JSON 或其他占位物冒充 Cubism 产物。

```powershell
pwsh -NoProfile -File scripts/Test-CagArtAssets.ps1
cargo test --test cag_art_contracts
```
