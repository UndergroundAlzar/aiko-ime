# CAG Operator 建模与验收清单

本文是 `aiko_cag` 的人工交付清单。机器校验入口：

```powershell
pwsh -NoProfile -File scripts/Test-CagArtAssets.ps1
cargo test --test cag_art_contracts
```

当前状态只能是 `concept_only`。仓库没有真实 PSD、`.cmo3`、`.moc3` 或
`.model3.json`，不得创建空文件、改后缀文件或手写文件冒充。

## 1. 参考图与画布

- [ ] 以 `source/reference.json` 指向的 `aiko_cag_master_source.png` 和
  `aiko_cag_master_cutout.png` 为唯一平面参考。
- [ ] PSD 画布固定 1024 x 1536、RGB、8 bit/通道、透明背景。
- [ ] 所有分层 PNG 保留完整画布和统一原点，不逐层裁边。
- [ ] 角色左右以角色自身视角命名。
- [ ] 参考图发生有意修改时同步更新 SHA-256、清单和验收记录。
- [ ] 使用表情表校对 12 种表情和口型，使用动作表校对枪带张力与装备避让。
- [ ] 使用拆件参考核对组件边界；它不是可直接导入的 PSD 图层，遮挡区域仍须补绘。

## 2. 不可变造型

- [ ] GPNVG 必须有一个底座和四个独立镜筒：
  `gpnvg_tube_outer_l`、`gpnvg_tube_inner_l`、
  `gpnvg_tube_inner_r`、`gpnvg_tube_outer_r`。
- [ ] 四镜筒使用同一刚性父变形器。默认上翻；下放动作中数量、间距和顺序不变。
- [ ] 左右手均为全指手套，五指轮廓闭合；不得复用裸手或露出指尖。
- [ ] 武器为 KAC KS-1，左右枪带固定在 plate carrier 前方，枪口始终朝下。
- [ ] 手不得持枪。待机、点赞、挥手、摸头、困倦和错误动作都保持手与枪带分离。
- [ ] 胸贴文字只能是 `AIKO` 与 `B RH-`，两者各自独立 ArtMesh。
- [ ] 允许通讯耳机上的实体 `boom_mic`；禁止麦克风图标、麦克风贴章、背景麦克风
  标志和软件品牌标志。

## 3. 分层

- [ ] `source/layers.csv` 中每个 `layer_id` 对应同名 `png/parts/<layer_id>.png`。
- [ ] `source/layers-addon.csv` 仅为战术装备子集，字段必须与完整表完全一致。
- [ ] 头发、脸、耳朵、颈部、躯干、作战服、手臂和腿保留装备背后的完整底图。
- [ ] 头盔、GPNVG、耳机、胸挂、附件、枪带、枪体和投影分别独立。
- [ ] `AIKO`、`B RH-` 不烘焙进胸挂底图。
- [ ] `boom_mic` 不与脸或头发合层。
- [ ] 枪体为硬表面网格；枪带、通讯线和腰包不能与枪体共用物理组。

## 4. 参数与物理

- [ ] 实现 `source/parameters.csv` 中全部 `required=true` 参数。
- [ ] 头部三轴同时绑定头盔、GPNVG 和耳机，转头时装备不漂移。
- [ ] `ParamCagGpnvgDeploy` 的 0 为上翻，1 为下放；四镜筒只做刚性联动。
- [ ] `ParamCagRifleX/Y` 只允许低幅度惯性，不能产生布料式弯曲。
- [ ] 左右枪带、通讯线和腰包使用独立低幅度物理组。
- [ ] 身体三轴和呼吸极值下枪口仍朝下，枪体不穿胸挂、腰带或左腿。
- [ ] 头发物理极值下不穿头盔、耳罩、GPNVG 底座或杆麦。
- [ ] 倾听状态使用眼神和姿态表达，不增加麦克风发光或图标参数。

## 5. 动作

- [ ] 实现 `source/motions.csv` 中全部 `required=true` 动作。
- [ ] 每个动作验收 `hand_pose=*_clear` 和
  `weapon_pose=chest_slung_muzzle_down`。
- [ ] `success` 点赞和 `wave` 挥手时，活动手避开枪、枪带和胸贴。
- [ ] `petted` 的头部位移不使头盔、四镜筒、耳机与头发分离。
- [ ] `sleepy` 下沉和 `error` 收肩不改变枪口方向。
- [ ] 可选 `gear_check` 下放 GPNVG 后必须回到默认上翻。

## 6. 表情

- [ ] 实现 `source/expressions.csv` 中全部 `required=true` 表情。
- [ ] 表情只驱动脸、眼、眉、嘴和必要的头部姿态。
- [ ] 所有表情保持 `AIKO` 与 `B RH-` 可读。
- [ ] 所有表情 `uses_icon=false`；不得增加麦克风或录音图标。
- [ ] 困倦、错误和摸头表情不遮挡胸贴或改变装备层级。

## 7. 遮挡补绘

- [ ] 完成 `source/occlusion-redraw.csv` 中全部 `required=true` 项。
- [ ] 临时隐藏每个 occluder，检查 back layer 无透明洞、断边或突变纹理。
- [ ] 在参数极值和物理极值重复检查，不只检查默认姿势。
- [ ] 头盔下头发、耳罩下耳朵、杆麦下脸、下巴带下颈部必须补全。
- [ ] 胸挂与附件后的躯干、作战服和胸挂织物必须补全。
- [ ] 枪与枪带后的腰带、裤装、左腿和枪体表面必须补全。
- [ ] 左右手五指在挥手、点赞和放松姿势下均保持全指覆盖。

## 8. Cubism 交付门槛

- [ ] 分层 PSD 完成后再创建 `source/aiko_cag_master.psd` 并把 source 状态改为
  `ready`。
- [ ] `.cmo3` 必须由 Cubism Editor 保存到 `ready/aiko_cag.cmo3`。
- [ ] `.moc3`、纹理和 `.model3.json` 必须由 Cubism Editor 正式导出到 `runtime/`。
- [ ] runtime 引用闭包完整，且无未引用临时文件、源 PSD 或备份。
- [ ] 通过 PowerShell 与 Rust 校验后，才可声明相应阶段 ready。
