# Aiko Live2D 资产区

本目录只存放 Aiko 的 Live2D 建模规范与后续真实模型资产。当前状态：

- `source`：仅有分层、参数、触摸区域、动作和表情规范，尚无真实 PSD/PNG。
- `ready`：尚无 Cubism Editor 工程，不存在可交接的 `.cmo3`。
- `runtime`：尚无 Cubism SDK 导出包，不存在 `.model3.json` 或 `.moc3`。

目录职责：

```text
assets/live2d/
|-- contracts/                 # Cubism JSON 文件的项目级 Schema
`-- aiko/
    |-- asset-manifest.json    # 阶段状态和入口声明
    |-- source/                # 美术源文件、命名表和建模输入
    |-- ready/                 # Cubism Editor 可继续编辑的工程
    `-- runtime/               # 应用可加载的 Cubism SDK 导出包
```

禁止用空文件、改后缀 JSON 或手写文件冒充 `.moc3`/`.cmo3`。阶段状态只有在真实资产通过
`scripts/Test-Live2DAssets.ps1` 对应严格检查后才能改成 `ready`。

完整制作和验收说明见 `docs/LIVE2D_MODELING.md`。

六套固定内置皮肤及其美术参考、模型 ID 和预留 runtime 入口统一记录在
`skin-catalog.json`。目录明确禁止任意模型导入；当前六套皮肤都有平面建模母版，但
Cubism runtime 仍为 `missing`，不能把这些 PNG 声称为可运行 Live2D 模型。
