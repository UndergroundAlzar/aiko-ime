# ready 阶段

这里用于存放可由 Cubism Editor 继续编辑和复核的真实工程，不是应用运行目录。

最小交付：

```text
ready/
|-- aiko.cmo3
|-- aiko.can3                 # 有独立动画工程时提供
|-- preview.png
`-- export-notes.md
```

`aiko.cmo3` 必须由 Cubism Editor 保存生成。不得用空文件、ZIP、JSON 或改后缀文件占位。
工程应完成参数范围、变形器、ArtMesh、物理、触摸网格、动作和表情检查，才能把
`asset-manifest.json` 中 `ready.status` 改为 `ready`。

当前没有真实 Cubism Editor 工程，状态为 `missing`。
