# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0
"""Draws the app's icon, the kit's logo in bread's amber on graphite, as
assets/icon.png and assets/icon.ico. Needs Pillow: python tools/icon.py"""

from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent.parent
SIZE = 1024
GROUND = (23, 26, 29, 255)  # Theme.chrome, dark
AMBER = (226, 167, 80, 255)  # Theme.brand, dark

# Icons.logo, on its 24 x 24 grid: a hexagon and the three edges to its middle.
OUTLINE = [(12, 2), (20, 6.5), (20, 15.5), (12, 20), (4, 15.5), (4, 6.5)]
SPOKES = [((12, 11), (20, 6.5)), ((12, 11), (12, 20)), ((12, 11), (4, 6.5))]


def draw() -> Image.Image:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, SIZE - 1, SIZE - 1), radius=SIZE * 0.2, fill=GROUND)
    scale = SIZE * 0.8 / 24
    off = (SIZE - 24 * scale) / 2, (SIZE - 22 * scale) / 2 - 2 * scale + SIZE * 0.04
    p = lambda x, y: (off[0] + x * scale, off[1] + y * scale)  # noqa: E731
    width = int(SIZE * 0.065)
    pts = [p(*q) for q in OUTLINE]
    d.line(pts + [pts[0], pts[1]], fill=AMBER, width=width, joint="curve")
    for a, b in SPOKES:
        d.line([p(*a), p(*b)], fill=AMBER, width=width)
    for q in pts + [p(12, 11)]:
        r = width / 2
        d.ellipse((q[0] - r, q[1] - r, q[0] + r, q[1] + r), fill=AMBER)
    return img


if __name__ == "__main__":
    img = draw()
    img.resize((256, 256), Image.LANCZOS).save(HERE / "assets" / "icon.png")
    img.save(HERE / "assets" / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
