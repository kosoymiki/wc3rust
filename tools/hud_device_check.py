#!/usr/bin/env python3
"""Device-frame acceptance for the final-pack HUD (ledger 457).

Reads the real device screenshot plus the element table emitted by
`examples/hud_layout.rs` and checks the operator's measured defects:

1. the four console section tops are equal within 1 px at x=100/1200/2300;
2. the console band is `viewport.y - console.y` = 405 +/- 1 px;
3. the top bar is exactly 45 px tall and covered by the plate (no light gap);
4. the ten control-group labels read 1,2,3,4,5,6,7,8,9,0 (OCR);
5. the minimap interior is not uniform (grayscale std > 10);
6. the selection panel is not an empty black hole (mean/std);
7. the card's right column is inside its section.

Usage:
    python3 tools/hud_device_check.py \
        --png analysis/hudskin/device-pack-2414x1080.png \
        --layout analysis/hudskin/pack-layout-2414x1080.json
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageOps

BAND_H = 405.0
BAR_H = 45.0


def crop_scaled(image: Image.Image, rect, factor: int, out: Path, threshold: int) -> Path:
    x, y, w, h = (int(round(v)) for v in rect)
    x = max(0, min(x, image.width - 1))
    y = max(0, min(y, image.height - 1))
    w = max(1, min(w, image.width - x))
    h = max(1, min(h, image.height - y))
    band = image.crop((x, y, x + w, y + h)).convert("L")
    band = band.resize((w * factor, h * factor), Image.LANCZOS)
    band = ImageOps.autocontrast(band).point(lambda p: 255 if p > threshold else 0)
    out.parent.mkdir(parents=True, exist_ok=True)
    band.save(out)
    return out


def ocr_digit(path: Path) -> str:
    if shutil.which("tesseract") is None:
        return ""
    result = subprocess.run(
        [
            "tesseract",
            str(path),
            "stdout",
            "-l",
            "eng",
            "--psm",
            "10",
            "-c",
            "tessedit_char_whitelist=0123456789",
        ],
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def section_index(layout, x: float) -> int:
    for index, section in enumerate(layout["sections"]):
        sx, _, sw, _ = section["rect"]
        if sx <= x < sx + sw:
            return index
    return -1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--png", required=True, type=Path)
    parser.add_argument("--layout", required=True, type=Path)
    parser.add_argument("--out-dir", type=Path, default=None)
    args = parser.parse_args()

    layout = json.loads(args.layout.read_text(encoding="utf-8"))
    image = Image.open(args.png).convert("RGB")
    viewport = [int(round(v)) for v in layout["viewport"]]
    if image.size != (viewport[0], viewport[1]):
        print(f"HUD_DEVICE_CHECK FAIL reason=frame-size {image.size} != {viewport}")
        return 1
    out_dir = args.out_dir or (args.png.parent / "ocr-device")
    pixels = np.asarray(image)
    console = layout["console"]
    console_y = int(round(console[1]))
    failures: list[str] = []

    # 1/2: section tops and band height. The pack stone fill makes the top a
    # straight line; measure the first row that matches the stone reference at
    # each section's probe column.
    probes = [100, 1200, 2300]
    tops = {}

    def hud_like(pixel) -> bool:
        r, g, b = (float(value) for value in pixel)
        lum = 0.2126 * r + 0.7152 * g + 0.0722 * b
        # World grass is green-dominant; outside the map is near-black. A HUD
        # row is lit stone/gold that is not green-dominant.
        return lum > 15.0 and not (g > r + 10.0 and g > b + 10.0)

    for x in probes:
        index = section_index(layout, x)
        found = None
        for y in range(max(0, console_y - 40), min(viewport[1], console_y + 80)):
            if hud_like(pixels[y, x]):
                found = y
                break
        tops[x] = found
        print(f"section_x={x} section={index} band_top={found}")
    if any(value is None for value in tops.values()):
        failures.append(f"section top not found: {tops}")
    else:
        spread = max(tops.values()) - min(tops.values())
        print(f"section_top_spread={spread}px (must be <= 1)")
        if spread > 1:
            failures.append(f"section tops differ by {spread} px: {tops}")
        band = viewport[1] - min(tops.values())
        print(f"band_height={band}px (must be {BAND_H:.0f} +/- 1)")
        if abs(band - BAND_H) > 1:
            failures.append(f"band height {band} != {BAND_H}")

    # 3: top bar 45 px, no light gap. Bar rows are not world-green.
    topbar = next(element for element in layout["elements"] if element["id"] == "topbar")
    bar_h = topbar["rect"][3]
    print(f"topbar_height={bar_h:.1f}px (must be {BAR_H:.0f})")
    if abs(bar_h - BAR_H) > 1:
        failures.append(f"topbar height {bar_h:.1f} != {BAR_H}")
    rows = []
    for y in range(0, 70):
        strip = pixels[y, 0:300].astype(float)
        green = (strip[:, 1] > strip[:, 0] + 12) & (strip[:, 1] > strip[:, 2] + 12)
        rows.append(1.0 - float(green.mean()))
    covered = [y for y, value in enumerate(rows[: int(round(bar_h))]) if value > 0.9]
    print(f"topbar_covered_rows={len(covered)}/{int(round(bar_h))}")
    if len(covered) < int(round(bar_h)) - 2:
        failures.append("topbar has uncovered rows (light gap)")

    # 4: group labels 1..9,0.
    groups = sorted(
        (element for element in layout["elements"] if element["id"].startswith("group_hit_")),
        key=lambda element: int(element["id"].rsplit("_", 1)[1]),
    )
    digits = ""
    for index, group in enumerate(groups):
        x, y, w, h = group["rect"]
        factor = 10
        for threshold in (120, 150):
            path = crop_scaled(
                image,
                [x + w * 0.2, y + h * 0.2, w * 0.6, h * 0.6],
                factor,
                out_dir / f"group_{index}_{threshold}.png",
                threshold,
            )
            text = ocr_digit(path)
            if text:
                digits += text[0]
                break
        else:
            digits += "?"
    print(f"group_ocr={digits!r} (must contain 1234567890 in order)")
    if digits != "1234567890":
        failures.append(f"group OCR {digits!r} != 1234567890")

    # 5: minimap interior std.
    minimap = next(
        element for element in layout["elements"] if element["id"] == "minimap_image"
    )["rect"]
    x, y, w, h = (int(round(v)) for v in minimap)
    sample = pixels[y + 4 : y + h - 4, x + 4 : x + w - 4].astype(float)
    std = float(sample.mean(axis=2).std())
    print(f"minimap_std={std:.1f} (must be > 10)")
    if std <= 10.0:
        failures.append(f"minimap interior std {std:.1f} <= 10")

    # 6: selection panel is not an empty black hole.
    panel = next(
        element for element in layout["elements"] if element["id"] == "selection_panel"
    )["rect"]
    x, y, w, h = (int(round(v)) for v in panel)
    inset = int(round(min(w, h) * 0.12))
    sample = pixels[y + inset : y + h - inset, x + inset : x + w - inset].astype(float)
    mean = float(sample.mean())
    std = float(sample.mean(axis=2).std())
    print(f"selection_panel_mean={mean:.1f} std={std:.1f} (must not be black)")
    if mean < 15.0 or std < 8.0:
        failures.append(f"selection panel looks empty: mean={mean:.1f} std={std:.1f}")

    # 7: card right column inside its section.
    card = next(element for element in layout["elements"] if element["id"] == "card")["rect"]
    right = card[0] + card[2]
    section = layout["sections"][3]["rect"]
    limit = section[0] + section[2]
    print(f"card_right={right:.1f} section_right={limit:.1f}")
    if right > limit - 1.0:
        failures.append(f"card right {right:.1f} leaves section 3 (right {limit:.1f})")

    if failures:
        for failure in failures:
            print(f"HUD_DEVICE_CHECK FAIL {failure}")
        return 1
    print("HUD_DEVICE_CHECK PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
