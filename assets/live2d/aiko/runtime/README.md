# runtime 阶段

这里仅存放应用可加载的 Cubism SDK 导出包。目标结构：

```text
runtime/
|-- aiko.model3.json
|-- aiko.moc3
|-- aiko.physics3.json
|-- aiko.cdi3.json
|-- textures/
|   |-- texture_00.png
|   `-- texture_01.png
|-- motions/
|   `-- *.motion3.json
`-- expressions/
    `-- *.exp3.json
```

规则：

- `aiko.model3.json` 是唯一运行时入口，所有引用必须是相对路径且留在本目录内。
- `aiko.moc3` 必须由 Cubism Editor 正式导出，文件头为 `MOC3`；禁止手工伪造。
- 纹理使用透明 PNG，文件名为小写 ASCII `snake_case`，不得依赖绝对路径。
- 动作组、表情、参数、触摸区域必须与 `source/*.csv` 契约一致。
- runtime 不接收 PSD、`.cmo3`、临时预览、备份和未被入口引用的资产。

当前没有真实 SDK 导出包，状态为 `missing`。
