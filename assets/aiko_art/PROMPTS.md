# Aiko Art Prompt Notes

这些图片使用 Codex 内置 image generation 工具生成。核心角色约束始终为：

- 白色妹妹头、齐刘海、单根呆毛、红瞳。
- 白/黑/红的 Aiko 视觉体系。
- 高质量二次元游戏角色设定图，保持统一脸型和比例。
- 不使用水印；建模母版使用纯绿色背景后本地去背。

本批次包含以下提示方向：

1. 正面 Live2D 母版：正面中性 A-pose、无遮挡、四肢与发片容易分层。
2. 表情口型表：12 种情绪，覆盖闭口、A、I、U/O、E 等口型。
3. 头部角度表：左右转头、抬头、低头、侧倾和视线移动。
4. 动作表：挥手、倾听、思考、点赞、道歉、打哈欠和摸头反馈。
5. 皮肤：居家睡衣、雨天、冬装、庆祝与 CAG Operator。
   - 睡衣、雨天、冬装、庆祝均另有正面 A-pose 绿幕母版和本地去背透明版。
   - 每张母版保持双手可见、四肢离开身体、配件可独立拆层，供 PSD 制作参考。
6. CAG Operator：
   - 现代 Multicam、低轮廓 plate carrier、高切盔和通讯耳机。
   - 四筒 GPNVG、全指手套。
   - KAC KS-1 风格步枪挂在胸前，枪口朝下，不手持。
   - `AIKO` 与 `B RH-` 胸贴。
   - 无麦克风贴章、无背景麦克风图标。
   - 表情表覆盖 12 种状态，动作表覆盖待机、挥手、敬礼、查看腕部设备、
     指向屏幕、蹲姿警戒、害羞和庆祝。
   - 拆件参考分别展示头盔、四筒 GPNVG、头发、五官、胸挂、全指手套、
     KAC KS-1 与枪带，供 PSD 分层时校对。

这些提示是可复现的创作规范，不替代 Cubism Editor 中的分层、网格、参数、
物理、动作和正式导出流程。

## 最终默认拆件表提示词

以下是 `modeling/aiko_default_parts_reference.png` 使用的完整提示词：

```text
Create a landscape Live2D modeling parts reference sheet for Aiko. Original polished anime game
character design, crisp clean line art and flat cel colors on a plain light-gray background. Aiko
is a cute girl with a short white bob, one small ahoge, red eyes, and a black-red rectangular hair
clip. Default outfit: oversized white hooded jacket with black panels and vivid red piping/straps,
black pleated skirt, black belt, white-black-red high-top shoes. On the left show one complete
front-facing relaxed A-pose. On the right arrange separated, non-overlapping component drawings:
front hair, back hair, side locks, ahoge, hair clip, open/half/closed eyes, eyebrows, mouth phonemes
A I U E O, smile, pout, hood, jacket front/back, sleeves, hands, skirt panels, belt, red straps,
socks and shoes. No labels or readable text, no weapons, no microphone symbol, no watermark, no
scenery, no dramatic lighting. Keep every part fully visible, uncropped, consistently designed,
and generously spaced.
```

生成方式：Codex 内置 image generation 工具。原始生成文件保留在 Codex 的
`generated_images` 目录，项目内最终副本为
`modeling/aiko_default_parts_reference.png`。
