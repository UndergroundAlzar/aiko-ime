"""Build the six-skin Aiko catalog from transparent modeling masters."""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[1]
ART_ROOT = ROOT / "assets" / "aiko_art"
OUTPUT = ART_ROOT / "skins" / "aiko_skin_catalog_v2.png"

SKINS = [
    ("aiko_live2d_master_cutout.png", "默认 Aiko", "Default", "#ef476f"),
    ("aiko_pajama_master_cutout.png", "睡衣版", "Pajama", "#f78fb3"),
    ("aiko_rain_master_cutout.png", "雨天版", "Rain", "#42b8d5"),
    ("aiko_winter_master_cutout.png", "冬装版", "Winter", "#5677a6"),
    (
        "aiko_celebration_master_cutout.png",
        "庆祝版",
        "Celebration",
        "#e0a325",
    ),
    (
        "aiko_cag_master_cutout.png",
        "CAG 特种作战干员",
        "CAG Operator",
        "#65735b",
    ),
]


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    names = ["msyhbd.ttc", "msyh.ttc"] if bold else ["msyh.ttc", "segoeui.ttf"]
    for name in names:
        path = Path("C:/Windows/Fonts") / name
        if path.is_file():
            return ImageFont.truetype(path, size)
    return ImageFont.load_default()


def contain(image: Image.Image, width: int, height: int) -> Image.Image:
    result = image.copy()
    result.thumbnail((width, height), Image.Resampling.LANCZOS)
    return result


def main() -> None:
    width, height = 1920, 1240
    margin, gap, header = 64, 28, 142
    card_width = (width - margin * 2 - gap * 2) // 3
    card_height = (height - header - margin - gap) // 2

    canvas = Image.new("RGBA", (width, height), "#f5f7fa")
    draw = ImageDraw.Draw(canvas)
    draw.text(
        (64, 34),
        "Aiko IME · 六套固定皮肤",
        font=font(44, bold=True),
        fill="#20242b",
    )
    draw.text(
        (64, 92),
        "Six fixed built-in Aiko skins · Live2D modeling masters",
        font=font(24),
        fill="#626b78",
    )

    for index, (filename, label_zh, label_en, accent) in enumerate(SKINS):
        column = index % 3
        row = index // 3
        x = margin + column * (card_width + gap)
        y = header + row * (card_height + gap)
        draw.rounded_rectangle(
            (x, y, x + card_width, y + card_height),
            radius=8,
            fill="#ffffff",
            outline="#d9dee6",
            width=2,
        )
        draw.rectangle(
            (x, y + card_height - 8, x + card_width, y + card_height),
            fill=accent,
        )

        source = ART_ROOT / "modeling" / filename
        with Image.open(source).convert("RGBA") as original:
            character = contain(original, card_width - 48, card_height - 116)
        image_x = x + (card_width - character.width) // 2
        image_y = y + 12 + (card_height - 116 - character.height) // 2
        canvas.alpha_composite(character, (image_x, image_y))

        draw.text(
            (x + 28, y + card_height - 70),
            label_zh,
            font=font(28, bold=True),
            fill="#222831",
        )
        draw.text(
            (x + 28, y + card_height - 34),
            label_en,
            font=font(20),
            fill="#68717e",
        )

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    canvas.convert("RGB").save(OUTPUT, optimize=True)
    print(OUTPUT)


if __name__ == "__main__":
    main()
