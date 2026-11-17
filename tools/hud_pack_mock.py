#!/usr/bin/env python3
"""Compose the HUD pack into a device-resolution mock, from the manifest only.

Purpose: prove the *placement* before Rust integration. Every rect below is
derived from `manifest.json` (design units at design height 768) scaled by
`s = screen_h / 768`; nothing is hand-tuned except the section split, which is
fixed by the four console pieces (each is a framed panel, measured in
`compose-A/B/C` experiments).

Output: `analysis/hudskin/pack-mock-<w>x<h>.png` + a printed table of rects.

usage: hud_pack_mock.py <pack_dir> <out.png> [--portrait] [--bg image.png]
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

from PIL import Image

DESIGN_H = 768.0


def nine(img: Image.Image, w: int, h: int, ins: dict) -> Image.Image:
    l, t, r, b = ins["l"], ins["t"], ins["r"], ins["b"]
    sw, sh = img.size
    if l + r >= sw or t + b >= sh:
        # Manifest defect (icons/arrows/header): draw unsliced.
        return img.resize((max(1, w), max(1, h)), Image.LANCZOS)
    xs_dst = [0, l, max(l, w - r), w]
    ys_dst = [0, t, max(t, h - b), h]
    xs_src = [0, l, max(l, sw - r), sw]
    ys_src = [0, t, max(t, sh - b), sh]
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    for iy in range(3):
        for ix in range(3):
            sx0, sx1 = xs_src[ix], xs_src[ix + 1]
            sy0, sy1 = ys_src[iy], ys_src[iy + 1]
            dx0, dx1 = xs_dst[ix], xs_dst[ix + 1]
            dy0, dy1 = ys_dst[iy], ys_dst[iy + 1]
            if sx1 <= sx0 or sy1 <= sy0 or dx1 <= dx0 or dy1 <= dy0:
                continue
            tile = img.crop((sx0, sy0, sx1, sy1)).resize((dx1 - dx0, dy1 - dy0), Image.BILINEAR)
            out.paste(tile, (dx0, dy0), tile)
    return out


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    pack = Path(sys.argv[1])
    out_path = Path(sys.argv[2])
    portrait = "--portrait" in sys.argv
    bg_path = None
    if "--bg" in sys.argv:
        bg_path = Path(sys.argv[sys.argv.index("--bg") + 1])

    manifest = json.loads((pack / "manifest.json").read_text())
    assets = {a["file"]: a for a in manifest["assets"]}

    def img(name: str) -> Image.Image:
        return Image.open(pack / name).convert("RGBA")

    def ins(name: str) -> dict:
        return assets[name]["insets"]

    w, h = (1080, 2414) if portrait else (2414, 1080)
    # Scale by the short side: a 1080x2414 portrait phone must not get a 905 px band.
    s = min(w, h) / DESIGN_H
    # Console sections (measured: four framed panels, not a continuous band).
    band = int(round(288 * s))
    ratios = [560, 760, 360, 520]
    widths = [int(round(w * r / sum(ratios))) for r in ratios]
    widths[-1] = w - sum(widths[:-1])

    canvas = Image.new("RGBA", (w, h), (18, 26, 16, 255))
    if bg_path and bg_path.is_file():
        canvas.alpha_composite(Image.open(bg_path).convert("RGBA").resize((w, h)), (0, 0))

    top = h - band
    sections = ["console/console_left_cap_normal_256x288.png",
                "console/console_left_mid_normal_256x288.png",
                "console/console_right_mid_normal_256x288.png",
                "console/console_right_cap_normal_256x288.png"]
    x = 0
    rects = []
    for name, width in zip(sections, widths):
        panel = nine(img(name), width, band, ins(name))
        canvas.alpha_composite(panel, (x, top))
        rects.append((name.rsplit("/", 1)[-1].split("_normal")[0], x, top, width, band))
        x += width

    def put(name: str, x: int, y: int, ww: int, hh: int, tint=None):
        piece = nine(img(name), ww, hh, ins(name))
        if tint:
            piece = Image.eval(piece, lambda v: v)
        canvas.alpha_composite(piece, (x, y))
        rects.append((name.rsplit("/", 1)[-1], x, y, ww, hh))

    # Section 1: minimap + header + control groups.
    mm = int(round(216 * s))
    put("minimap/minimap_frame_normal_216.png", widths[0] // 2 - mm // 2, top + 40, mm, mm)
    put("minimap/minimap_header_normal_216x20.png", widths[0] // 2 - mm // 2, top + 40 - int(20 * s), mm, int(20 * s))
    g = int(round(44 * s))
    gap = int(round(6 * s))
    # Groups live in section 2 (5x2), left of the portrait: section 1 is exactly
    # the minimap + header after the 32 px insets (28 + 304 = 332 <= 341).
    grid_w = 5 * g + 4 * gap
    grid_h = 2 * g + gap
    gx = widths[0] + 32
    gy0 = top + (band - grid_h) // 2
    for i in range(10):
        cx = gx + (i % 5) * (g + gap)
        cy = gy0 + (i // 5) * (g + gap)
        put("controlgroups/control_group_button_inactive_44.png", cx, cy, g, g)

    # Section 2: portrait + unit info.
    p = int(round(128 * s))
    px = widths[0] + 32 + grid_w + 16
    put("portrait/portrait_frame_normal_128.png", px, top + (band - p) // 2, p, p)

    # Section 3: inventory 2x3.
    inv_x0 = widths[0] + widths[1]
    slot_w = (widths[2] - 64 - 8) // 2
    slot_h = (band - 64 - 2 * 8) // 3
    inv_w = 2 * slot_w + 8
    inv_h = 3 * slot_h + 16
    inv_x = inv_x0 + (widths[2] - inv_w) // 2
    inv_y = top + (band - inv_h) // 2
    for i in range(6):
        cx = inv_x + (i % 2) * (slot_w + 8)
        cy = inv_y + (i // 2) * (slot_h + 8)
        put("commandcard/button_border_normal_64.png", cx, cy, slot_w, slot_h)

    # Section 4: command card 4x3.
    card_x0 = widths[0] + widths[1] + widths[2]
    cell = int(round(64 * s))
    cgap = int(round(4 * s))
    grid_w = 4 * cell + 3 * cgap
    grid_h = 3 * cell + 2 * cgap
    gx = card_x0 + (widths[3] - grid_w) // 2
    gy = top + (band - grid_h) // 2
    for i in range(12):
        cx = gx + (i % 4) * (cell + cgap)
        cy = gy + (i // 4) * (cell + cgap)
        put("commandcard/button_border_normal_64.png", cx, cy, cell, cell)

    # Top strip: plate tiled, resource icons.
    plate_name = "topbar/topbar_plate_normal_720x32.png"
    strip_h = int(round(32 * s))
    canvas.alpha_composite(nine(img(plate_name), w, strip_h, ins(plate_name)), (0, 0))
    icon = int(round(32 * s))
    for i, name in enumerate(["resources/icon_gold_normal_32.png",
                              "resources/icon_lumber_normal_32.png",
                              "resources/icon_food_normal_32.png"]):
        canvas.alpha_composite(img(name).resize((icon, icon)), (w // 2 + i * (icon * 3), 4))

    out_path.parent.mkdir(parents=True, exist_ok=True)
    canvas.convert("RGB").save(out_path)
    print(f"MOCK {out_path} {w}x{h} scale={s:.3f} band={band} sections={widths}")
    for name, rx, ry, rw, rh in rects:
        print(f"  {name:<46} {rx:>5} {ry:>5} {rw:>5} {rh:>4}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
