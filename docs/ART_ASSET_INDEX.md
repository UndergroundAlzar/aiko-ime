# Aiko Art Asset Index / Aiko 美术资产索引

更新日期 / Updated: 2026-06-11

## 结论 / Conclusion

**当前规划的生图已经全部完成；后续重点是人工分层、Cubism 建模和真实运行验证。**
All planned image generation is complete; the next focus is human layering, Cubism modeling, and real runtime validation.

当前工作区可见 **65** 个图像文件；其中源码资产树 `assets/` 下有 **64** 个图像文件。
仓库根目录忽略的临时生成图，以及 `.omc/`、`dist/`、`target/`、`portable_aiko_test/`
和本地 `config.toml` 属于本地生成、构建、测试或运行状态，不是美术源文件，也不应作为
GitHub 或 Release 发布输入。

状态词：

- **运行资产 / runtime asset**：当前程序直接使用或构建时使用。
- **发布副本 / release copy**：为稳定路径保留的复制件。
- **平面参考 / flat reference**：供人工分层、补绘或建模校对，不是 Live2D 模型。
- **展示资产 / presentation asset**：README、目录或宣传展示用途。
- **源图 / source image** 与 **透明切图 / cutout**：都仍是单张平面图。

## 根目录运行与发布资产 / Root Runtime And Release Assets

| 文件 / File | 尺寸 / Size | 用途 / Purpose |
|---|---:|---|
| `assets/aiko_app_icon.ico` | 256x256 | Windows EXE 图标，`build.rs` 使用。Windows executable icon used by `build.rs`. |
| `assets/aiko_app_icon.png` | 1024x1024 | 应用图标发布副本；内容与 `assets/mascot/aiko_app_icon.png` 相同。Release copy of the application icon. |
| `assets/aiko_desktop_pet.png` | 256x256 | 当前 PNG 桌宠 idle 帧，`desktop_pet.rs` 直接嵌入；与 `aiko_desktop_pet_256.png` 相同。Current embedded idle sprite. |
| `assets/aiko_readme_showcase.png` | 1200x630 | README 顶部展示图；与 mascot 目录副本相同。README showcase banner. |
| `assets/aiko_tray_icon.png` | 32x32 | 当前系统托盘图标，`system_tray.rs` 直接嵌入。Current embedded tray icon. |
| `assets/icon_idle.png` | 48x48 | 语音浮动按钮 idle 状态图。Voice floating-button idle icon. |
| `assets/icon_processing.png` | 48x48 | 语音浮动按钮 processing 状态图。Voice floating-button processing icon. |
| `assets/icon_recording.png` | 48x48 | 语音浮动按钮 recording 状态图。Voice floating-button recording icon. |

## Live2D 平面建模参考 / Flat Modeling References

这些文件全部已生成，但都不是 PSD、CMO3、MOC3、纹理图集或可运行模型。
These files are complete flat references, not Cubism deliverables.

| 文件 / File | 尺寸 / Size | 用途 / Purpose |
|---|---:|---|
| `assets/aiko_art/modeling/aiko_live2d_master_source.png` | 1024x1536 | 默认 Aiko 正面母版，造型和配色基准。Default front master for design and color. |
| `assets/aiko_art/modeling/aiko_live2d_master_cutout.png` | 1024x1536 | 默认母版透明轮廓参考；不能直接绑定。Transparent silhouette reference, not bind-ready layers. |
| `assets/aiko_art/modeling/aiko_expression_sheet.png` | 1448x1086 | 默认版 12 种表情、眼眉和口型参考。Default expression and mouth-shape sheet. |
| `assets/aiko_art/modeling/aiko_head_angle_sheet.png` | 1254x1254 | 默认版左右转头、抬低头、侧倾和视线透视参考。Head-angle and gaze perspective sheet. |
| `assets/aiko_art/modeling/aiko_motion_sheet.png` | 1536x1024 | 默认版待机、倾听、处理、成功、错误、摸头和困倦动作参考。Default state-motion reference. |
| `assets/aiko_art/modeling/aiko_default_parts_reference.png` | 1536x1024 | 默认服装的发片、五官、口型、衣服正反面、袖子、手、裙片、腰带、挂带和鞋类拆件校对图；不是实际图层。Default parts-separation reference, not actual layers. |
| `assets/aiko_art/modeling/aiko_pajama_master_source.png` | 1023x1537 | 睡衣皮肤正面平面母版。Pajama skin flat front master. |
| `assets/aiko_art/modeling/aiko_pajama_master_cutout.png` | 1023x1537 | 睡衣皮肤透明轮廓参考。Pajama transparent cutout reference. |
| `assets/aiko_art/modeling/aiko_rain_master_source.png` | 1024x1536 | 雨天皮肤正面平面母版。Rain skin flat front master. |
| `assets/aiko_art/modeling/aiko_rain_master_cutout.png` | 1024x1536 | 雨天皮肤透明轮廓参考。Rain transparent cutout reference. |
| `assets/aiko_art/modeling/aiko_winter_master_source.png` | 1024x1536 | 冬装皮肤正面平面母版。Winter skin flat front master. |
| `assets/aiko_art/modeling/aiko_winter_master_cutout.png` | 1024x1536 | 冬装皮肤透明轮廓参考。Winter transparent cutout reference. |
| `assets/aiko_art/modeling/aiko_celebration_master_source.png` | 971x1619 | 庆祝皮肤正面平面母版。Celebration skin flat front master. |
| `assets/aiko_art/modeling/aiko_celebration_master_cutout.png` | 971x1619 | 庆祝皮肤透明轮廓参考。Celebration transparent cutout reference. |
| `assets/aiko_art/modeling/aiko_cag_master_source.png` | 1024x1536 | CAG Operator 正面母版；装备与配色主参考。CAG Operator front master. |
| `assets/aiko_art/modeling/aiko_cag_master_cutout.png` | 1024x1536 | CAG Operator 透明轮廓参考。CAG transparent cutout reference. |
| `assets/aiko_art/modeling/aiko_cag_expression_sheet.png` | 1448x1086 | CAG 版 12 种表情与口型参考。CAG expression and mouth-shape sheet. |
| `assets/aiko_art/modeling/aiko_cag_motion_sheet.png` | 1581x995 | CAG 动作、枪带张力、装备避让与枪口方向参考。CAG motion and equipment-clearance sheet. |
| `assets/aiko_art/modeling/aiko_cag_parts_reference.png` | 1536x1024 | 头盔、四筒 GPNVG、头发、五官、胸挂、手套、枪和枪带拆件参考；不是实际图层。CAG parts-separation reference, not actual layers. |

## 皮肤目录图 / Skin Catalog Images

| 文件 / File | 尺寸 / Size | 用途 / Purpose |
|---|---:|---|
| `assets/aiko_art/skins/aiko_skin_lineup.png` | 1774x887 | 早期四套皮肤概念排布：睡衣、雨天、冬装、庆祝。Early four-skin lineup. |
| `assets/aiko_art/skins/aiko_skin_catalog.png` | 2200x1050 | 五皮肤目录图，包含 CAG Operator；历史展示资产。Five-skin historical catalog. |
| `assets/aiko_art/skins/aiko_skin_catalog_v2.png` | 1920x1240 | 与程序固定 ID 一致的六皮肤总览，README 当前使用。Current six-skin catalog used by README. |

## 展示插画 / Presentation Illustrations

| 文件 / File | 尺寸 / Size | 用途 / Purpose |
|---|---:|---|
| `assets/aiko_art/illustrations/aiko_headpat_desktop.png` | 1672x941 | 摸头桌面场景展示；不进入模型 runtime。Head-pat desktop key art. |
| `assets/aiko_art/illustrations/aiko_sleepy_bubble.png` | 1672x941 | 困倦/睡眠场景展示。Sleepy-state presentation art. |
| `assets/aiko_art/illustrations/aiko_rainy_day.png` | 1024x1536 | 雨天皮肤场景展示。Rain-skin presentation art. |
| `assets/aiko_art/illustrations/aiko_winter_desktop.png` | 1672x941 | 冬装桌面场景展示。Winter desktop presentation art. |
| `assets/aiko_art/illustrations/aiko_release_celebration.png` | 864x1821 | 发布庆祝场景展示。Release-celebration presentation art. |
| `assets/aiko_art/illustrations/aiko_cag_operator_keyart.png` | 1672x941 | CAG Operator 主视觉展示。CAG Operator key art. |

## Mascot 基础源图与派生图 / Mascot Sources And Derivatives

| 文件 / File | 尺寸 / Size | 用途 / Purpose |
|---|---:|---|
| `assets/mascot/aiko_icon_source.png` | 1254x1254 | 应用图标的原始平面源图。Flat source image for the app icon. |
| `assets/mascot/aiko_icon_cutout.png` | 992x1230 | 图标角色透明切图，用于图标派生。Transparent icon-character cutout. |
| `assets/mascot/aiko_app_icon.png` | 1024x1024 | 应用图标主 PNG；根目录有相同发布副本。Primary app-icon PNG. |
| `assets/mascot/aiko_pet_source.png` | 1536x1024 | 桌宠 idle 造型原始平面图。Flat source for the idle desktop pet. |
| `assets/mascot/aiko_pet_cutout.png` | 1106x960 | 桌宠 idle 透明切图，用于生成方形运行帧。Transparent idle-pet cutout. |
| `assets/mascot/aiko_desktop_pet_512.png` | 512x512 | 512px 桌宠派生图，展示/高分辨率备用。512px desktop-pet derivative. |
| `assets/mascot/aiko_desktop_pet_256.png` | 256x256 | 256px idle 运行帧；与根目录运行副本相同。256px idle runtime source copy. |
| `assets/mascot/aiko_tray_icon_64.png` | 64x64 | 托盘图标高分辨率源；根目录 32px 文件为运行版本。64px tray-icon source. |
| `assets/mascot/aiko_readme_showcase.png` | 1200x630 | README 展示图主副本；根目录有相同发布副本。Primary showcase copy. |

## 当前 PNG 桌宠状态帧 / Current PNG Pet State Frames

这些 512x512 文件中，除 idle 使用根目录 `assets/aiko_desktop_pet.png` 外，其余六张由
`src/ui/desktop_pet.rs` 直接嵌入。它们是当前可运行桌宠，不是 Live2D。

| 文件 / File | 尺寸 / Size | 用途 / Purpose |
|---|---:|---|
| `assets/mascot/states/aiko_listening.png` | 512x512 | 正在收音状态帧。Listening sprite. |
| `assets/mascot/states/aiko_processing.png` | 512x512 | 正在处理状态帧。Processing sprite. |
| `assets/mascot/states/aiko_success.png` | 512x512 | 成功反馈状态帧。Success sprite. |
| `assets/mascot/states/aiko_error.png` | 512x512 | 错误反馈状态帧。Error sprite. |
| `assets/mascot/states/aiko_petted.png` | 512x512 | 摸头开心状态帧。Petted sprite. |
| `assets/mascot/states/aiko_sleepy.png` | 512x512 | 困倦状态帧。Sleepy sprite. |
| `assets/mascot/states/aiko_states_preview.png` | 768x576 | 多状态联系表/人工预览，不被程序嵌入。State contact sheet for review. |

## 状态图原始生成图 / State Source Images

| 文件 / File | 尺寸 / Size | 用途 / Purpose |
|---|---:|---|
| `assets/mascot/states/source/aiko_listening_source.png` | 1024x1536 | listening 状态原始平面生成图。Listening source image. |
| `assets/mascot/states/source/aiko_processing_source.png` | 1448x1086 | processing 状态原始平面生成图。Processing source image. |
| `assets/mascot/states/source/aiko_success_source.png` | 1536x1024 | success 状态原始平面生成图。Success source image. |
| `assets/mascot/states/source/aiko_error_source.png` | 1536x1024 | error 状态原始平面生成图。Error source image. |
| `assets/mascot/states/source/aiko_petted_source.png` | 1448x1086 | petted 状态原始平面生成图。Petted source image. |
| `assets/mascot/states/source/aiko_sleepy_source.png` | 1448x1086 | sleepy 状态原始平面生成图。Sleepy source image. |

## 状态透明切图 / State Cutouts

| 文件 / File | 尺寸 / Size | 用途 / Purpose |
|---|---:|---|
| `assets/mascot/states/cutout/aiko_listening.png` | 1024x1536 | listening 透明中间资产，用于制作 512px 状态帧。Listening transparent intermediate. |
| `assets/mascot/states/cutout/aiko_processing.png` | 1448x1086 | processing 透明中间资产。Processing transparent intermediate. |
| `assets/mascot/states/cutout/aiko_success.png` | 1536x1024 | success 透明中间资产。Success transparent intermediate. |
| `assets/mascot/states/cutout/aiko_error.png` | 1536x1024 | error 透明中间资产。Error transparent intermediate. |
| `assets/mascot/states/cutout/aiko_petted.png` | 1448x1086 | petted 透明中间资产。Petted transparent intermediate. |
| `assets/mascot/states/cutout/aiko_sleepy.png` | 1448x1086 | sleepy 透明中间资产。Sleepy transparent intermediate. |

## 六套皮肤与 Live2D 状态 / Six Skins And Live2D Status

| 皮肤 / Skin | 平面参考 / Flat art | 预留入口 / Reserved runtime entry | 当前 Cubism 状态 |
|---|---|---|---|
| 默认 / Default | source + cutout + 表情/角度/动作表均完成 | `assets/live2d/aiko/runtime/aiko.model3.json` | `source=spec_only`, `ready=missing`, `runtime=missing` |
| 睡衣 / Pajama | source + cutout 完成 | `assets/live2d/aiko_pajama/runtime/aiko_pajama.model3.json` | 仅预留路径，runtime missing |
| 雨天 / Rain | source + cutout 完成 | `assets/live2d/aiko_rain/runtime/aiko_rain.model3.json` | 仅预留路径，runtime missing |
| 冬装 / Winter | source + cutout 完成 | `assets/live2d/aiko_winter/runtime/aiko_winter.model3.json` | 仅预留路径，runtime missing |
| 庆祝 / Celebration | source + cutout 完成 | `assets/live2d/aiko_celebration/runtime/aiko_celebration.model3.json` | 仅预留路径，runtime missing |
| CAG Operator | source + cutout + 表情/动作/拆件参考完成 | `assets/live2d/aiko_cag/runtime/aiko_cag.model3.json` | `source=concept_only`, `ready=missing`, `runtime=missing` |

## 使用规则 / Usage Rules

1. 不再生图。Use the existing files; do not generate more art.
2. 不把 source/cutout/sheet 直接放入 Cubism runtime。
3. 不把透明 cutout 当作完整分层 PSD。
4. 分层与遮挡补绘必须人工完成，并以 CSV 契约校对。
5. 默认模型参考文件受
   `assets/live2d/aiko/source/production-manifest.json` 的尺寸和 SHA-256 固定。
6. CAG 参考文件受 `assets/live2d/aiko_cag/source/reference.json` 固定，并必须保持
   四筒 GPNVG、全指手套、胸挂枪口向下、`AIKO` / `B RH-` 胸贴等不变量。
7. 发布副本与主副本有意重复，不要在不检查构建和 README 引用的情况下删除。

## English Summary

The workspace currently contains 65 visible image files; the source-controlled art inventory under
`assets/` contains 64 image files. Ignored root-level temporary generated images, `.omc/`, `dist/`,
`target/`, `portable_aiko_test/`, and local `config.toml` are local generation, build, test, or
runtime state, not GitHub or Release inputs. The current runtime uses PNG sprites. Modeling masters,
cutouts, expression sheets, motion sheets, catalogs, and illustrations are flat references or
presentation assets. None of them is a layered PSD, Cubism project, MOC3 runtime, or texture atlas.
Human separation, redraw, rigging, Editor export, and official SDK validation remain required.
