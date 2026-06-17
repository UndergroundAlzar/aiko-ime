# CAG source contract

本目录当前只包含建模契约，不包含伪造的分层源文件或 Cubism 文件。

交付真实源资产时：

1. 新建 `aiko_cag_master.psd`，画布固定为 1024 x 1536、RGB、8 bit/通道和透明背景。
2. PSD 图层 ID 与 `layers.csv` 一致，左右方向以角色自身视角命名。
3. 导出的 `png/parts/*.png` 保留完整画布和统一原点，不逐层裁边。
4. `layers-addon.csv` 只保留 CAG 装备子集，且字段值与 `layers.csv` 完全一致。
5. 按 `occlusion-redraw.csv` 补全所有被遮挡底图，再进行网格和变形器绑定。
6. `.cmo3` 只放在 `../ready/`，`.moc3` 和 `.model3.json` 只放在 `../runtime/`。

辅助参考：

- `aiko_cag_expression_sheet.png` 用于表情和口型一致性，不作为纹理直接导入。
- `aiko_cag_motion_sheet.png` 用于动作、枪带张力和装备避让校对。
- `aiko_cag_parts_reference.png` 用于拆层检查，最终 PSD 仍须补绘遮挡区域。

`boom_mic` 表示头戴通讯设备的实体杆麦，不是图标或贴章。任何名为
`mic_icon`、`microphone_patch` 或同义视觉标识的图层都不允许加入。
