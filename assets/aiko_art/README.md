# Aiko Art Library / Aiko 美术资产库

本目录保存 Aiko 的建模参考、皮肤方案与展示插画。它们是美术源参考，不等同于
Cubism Editor 工程或可运行的 Live2D 模型。

## 目录

- `modeling/`
  - `aiko_live2d_master_source.png`：默认服装正面建模母版。
  - `aiko_live2d_master_cutout.png`：默认母版透明预览。
  - `aiko_expression_sheet.png`：12 种表情和口型参考。
  - `aiko_head_angle_sheet.png`：头部角度和视线参考。
  - `aiko_motion_sheet.png`：8 种上半身动作参考。
  - `aiko_default_parts_reference.png`：默认服装的发片、五官、口型、衣服正反面、
    袖子、手、裙片、腰带、挂带和鞋类拆件校对图。
  - `aiko_cag_master_source.png`：CAG Operator 皮肤正面建模母版。
  - `aiko_cag_master_cutout.png`：CAG 母版透明预览。
  - `aiko_cag_expression_sheet.png`：CAG 版 12 种表情与口型参考。
  - `aiko_cag_motion_sheet.png`：CAG 版 8 种全身动作与枪带避让参考。
  - `aiko_cag_parts_reference.png`：CAG 版分层拆件与装备组件参考。
  - `aiko_pajama_master_source.png` / `cutout.png`：睡衣版独立建模母版。
  - `aiko_rain_master_source.png` / `cutout.png`：雨天版独立建模母版。
  - `aiko_winter_master_source.png` / `cutout.png`：冬装版独立建模母版。
  - `aiko_celebration_master_source.png` / `cutout.png`：庆祝版独立建模母版。
- `skins/`
  - `aiko_skin_lineup.png`：居家、雨天、冬装、庆祝四套概念。
  - `aiko_skin_catalog.png`：包含 CAG Operator 的五皮肤目录图。
  - `aiko_skin_catalog_v2.png`：与程序一致的六套固定皮肤总览。
- `illustrations/`
  - 摸头、睡觉、雨天、冬日、庆祝和 CAG Operator 展示图。

## CAG Operator 约束

- 全指技术手套。
- 四个独立镜筒的 GPNVG，默认上翻以露出面部。
- 步枪通过枪带固定在 plate carrier 前方，枪口始终朝下，不作为手持道具。
- 胸前固定使用 `AIKO` 与 `B RH-` 信息贴。
- 不使用麦克风贴章、背景麦克风标志或软件品牌标志。

真正的 `.cmo3`、`.moc3`、`.model3.json` 和运行时纹理应放在
`assets/live2d/` 对应阶段目录，不能用这里的整张 PNG 冒充。
