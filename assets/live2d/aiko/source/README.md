# source 阶段

这里保存不可直接交给应用加载的美术源资产。

真实交付时应加入：

```text
source/
|-- aiko_master.psd
|-- png/
|   `-- parts/                 # 按 layers.csv 导出的透明 PNG
|-- layers.csv
|-- parameters.csv
|-- hit-areas.csv
|-- motions.csv
`-- expressions.csv
```

要求：

- `aiko_master.psd` 使用 RGB、8 bit/通道、透明背景，画布和像素密度在制作期锁定。
- PSD 图层名与 `layers.csv` 的 `layer_id` 一致；组名可带排序前缀，但交付 ID 不带前缀。
- 独立 PNG 必须保留与 PSD 相同画布、原点和透明边界，不得逐层自动裁边。
- 左右均以角色自身视角命名：`l` 是 Aiko 的左侧，`r` 是 Aiko 的右侧。
- 不在这里放 `.moc3`；`.moc3` 只能来自 Cubism Editor 的正式导出。

当前目录只有规范，`asset-manifest.json` 中 `source.status` 为 `spec_only`。
