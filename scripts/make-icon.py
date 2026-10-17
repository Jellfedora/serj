#!/usr/bin/env python3
"""Génère l'icône de Serj : une planche de BD dans la nuit, dont une seule case
est éclairée, comme pendant la lecture.

Écrit assets/icon/icon.png (1024 px, aussi l'icône de la fenêtre),
assets/icon/Serj.icns (macOS, demande iconutil) et assets/icon/serj.ico
(Windows). Demande Pillow.
"""
import shutil
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "icon"

SIZE = 1024
BG_TOP, BG_BOTTOM = (0x24, 0x24, 0x2C), (0x10, 0x10, 0x14)
PAPER = (0x3A, 0x3A, 0x44)
PANEL = (0x55, 0x55, 0x62)
GOLD_TOP, GOLD_BOTTOM = (0xFF, 0xD7, 0x7A), (0xE8, 0xA8, 0x3C)
INK = (0x14, 0x14, 0x18)


def vertical_gradient(size, top, bottom):
    grad = Image.new("RGB", (1, size[1]))
    for y in range(size[1]):
        t = y / (size[1] - 1)
        grad.putpixel((0, y), tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)))
    return grad.resize(size)


def rounded(box, radius):
    mask = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(mask).rounded_rectangle(box, radius, fill=255)
    return mask


def icon():
    # Tuile façon macOS : carré arrondi avec une marge, comme la grille d'Apple.
    margin, radius = 100, 185
    tile = rounded((margin, margin, SIZE - margin, SIZE - margin), radius)
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    shadow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    shadow.paste((0, 0, 0, 110), (0, 12), tile)
    img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(18)))
    bg = vertical_gradient((SIZE, SIZE), BG_TOP, BG_BOTTOM).convert("RGBA")
    bg.putalpha(tile)
    img.alpha_composite(bg)

    # La planche, un peu plus claire que la nuit.
    page = (262, 212, 762, 812)
    layer = Image.new("RGBA", (SIZE, SIZE), PAPER + (255,))
    layer.putalpha(rounded(page, 26))
    img.alpha_composite(layer)

    # Ses cases : une grande en haut, deux en bas ; celle de droite éclairée.
    gap, pad = 26, 34
    x0, y0, x1, y1 = page[0] + pad, page[1] + pad, page[2] - pad, page[3] - pad
    mid_y = y0 + (y1 - y0) * 0.44
    mid_x = x0 + (x1 - x0) * 0.42
    dim = [(x0, y0, x1, mid_y - gap / 2), (x0, mid_y + gap / 2, mid_x - gap / 2, y1)]
    lit = (mid_x + gap / 2, mid_y + gap / 2, x1, y1)
    for box in dim:
        layer = Image.new("RGBA", (SIZE, SIZE), PANEL + (255,))
        layer.putalpha(rounded(box, 12))
        img.alpha_composite(layer)

    # Le halo de la case lue, puis la case.
    glow = Image.new("RGBA", (SIZE, SIZE), GOLD_TOP + (0,))
    glow.putalpha(rounded(lit, 12).filter(ImageFilter.GaussianBlur(46)).point(lambda v: v * 0.7))
    img.alpha_composite(glow)
    gold = vertical_gradient((SIZE, SIZE), GOLD_TOP, GOLD_BOTTOM).convert("RGBA")
    gold.putalpha(rounded(lit, 12))
    img.alpha_composite(gold)

    # Une bulle dans la case éclairée.
    draw = ImageDraw.Draw(img)
    lx0, ly0, lx1, ly1 = lit
    bw, bh = (lx1 - lx0) * 0.62, (ly1 - ly0) * 0.26
    bx, by = lx0 + (lx1 - lx0 - bw) / 2, ly0 + (ly1 - ly0) * 0.2
    draw.ellipse((bx, by, bx + bw, by + bh), fill=(255, 255, 255, 255), outline=INK + (255,), width=8)
    tail = [(bx + bw * 0.3, by + bh * 0.85), (bx + bw * 0.22, by + bh * 1.45), (bx + bw * 0.5, by + bh * 0.95)]
    draw.polygon(tail, fill=(255, 255, 255, 255))
    draw.line([tail[0], tail[1], tail[2]], fill=INK + (255,), width=8, joint="curve")
    # Le bord de la bulle repeint par-dessus la base de la queue.
    draw.arc((bx, by, bx + bw, by + bh), 20, 160, fill=INK + (255,), width=8)
    draw.ellipse((bx + 6, by + 6, bx + bw - 6, by + bh - 6), fill=(255, 255, 255, 255))
    for k in range(3):
        cx = bx + bw * (0.3 + 0.2 * k)
        cy = by + bh / 2
        draw.ellipse((cx - 11, cy - 11, cx + 11, cy + 11), fill=INK + (255,))
    return img


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    img = icon()
    img.save(OUT / "icon.png")
    img.save(OUT / "serj.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    if shutil.which("iconutil"):
        with tempfile.TemporaryDirectory() as tmp:
            iconset = Path(tmp) / "Serj.iconset"
            iconset.mkdir()
            for s in (16, 32, 128, 256, 512):
                img.resize((s, s), Image.LANCZOS).save(iconset / f"icon_{s}x{s}.png")
                img.resize((s * 2, s * 2), Image.LANCZOS).save(iconset / f"icon_{s}x{s}@2x.png")
            subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(OUT / "Serj.icns")], check=True)


if __name__ == "__main__":
    main()
