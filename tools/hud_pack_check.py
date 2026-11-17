#!/usr/bin/env python3
"""Deterministic gate for the final-pack HUD frame (ledger 457).

Consumes the element table emitted by `cargo run -p wc3-game --example
hud_layout` plus the rendered PNG and re-derives every rule from
`manifest.json`:

1. completeness: 92 assets, 5 fonts, 20 atlas cells, file presence and PNG
   dimensions equal to the manifest;
2. the 11 rows whose insets cannot be sliced are classified unsliced (known
   defects, listed below with `# manifest-defect`) and, when drawn, their
   `HUD_PACK insets-ignored <file> (...)` log line must exist;
3. 9-slice: every sliced element's corner source size equals its manifest
   inset (corners native), and the drawn corner scales only with the design
   scale;
4. no single-image element is stretched beyond 1.15x of its authored device
   size (authored * design scale); centres/edges of 9-slices are exempt;
5. every interactive leaf is >= 90 px on its shorter side;
6. every text run is >= MIN_TEXT_PX (24 px);
7. no element leaves the viewport; floating panels stay inside the safe
   area, console/topbar content respects the horizontal safe margins (the
   pack contract pins the console to the physical bottom and the bar to the
   physical top);
8. the four console sections' widths sum to the viewport (heights to the
   console band in portrait), each section samples its own asset, and no
   interactive element crosses a section boundary;
9. no legacy look: the console band has real texture (unique colours,
   SSIM-lite against a constant field), no magenta wash, and is not the old
   procedural stone colour;
10. optional `--baseline` regression: SSIM-lite between two frames >= 0.90.

SSIM-lite is implemented here (global mean/variance/covariance with the
standard C1/C2 constants): `dssim` was rejected for licensing/build weight.

Usage:
    python3 tools/hud_pack_check.py \
      --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
      --layout analysis/hudskin/pack-layout-2414x1080.json \
      --layout analysis/hudskin/pack-layout-1080x2414.json \
      --png analysis/hudskin/pack-2414x1080.png \
      --png analysis/hudskin/pack-1080x2414.png \
      --log analysis/hudskin/pack-run.log
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

MIN_LEAF_PX = 90.0
MIN_TEXT_PX = 24.0
MAX_SINGLE_STRETCH = 1.15

# # manifest-defect: 2026-11-16 delivery. These rows declare insets that
# cannot be sliced (l+r >= w or t+b >= h); the art is drawn as one image and
# the loader logs `HUD_PACK insets-ignored <file> (...)` once per file. Flip
# this table when a pack revision fixes the numbers.
KNOWN_DEFECTS = {
    "minimap_header_normal_216x20",
    "icon_food_normal_32",
    "icon_food_warn_32",
    "icon_gold_normal_32",
    "icon_gold_warn_32",
    "icon_lumber_normal_32",
    "icon_lumber_warn_32",
    "upkeep_strip_roc_720x14",
    "upkeep_strip_tft_720x14",
    "tooltip_arrow_down_24x12",
    "tooltip_arrow_up_24x12",
}

SINGLE_PREFIXES = ("icon_", "tooltip_arrow_", "upkeep_strip_", "minimap_header_")


def insets(asset):
    value = asset["insets"]
    return [float(value["l"]), float(value["t"]), float(value["r"]), float(value["b"])]


def sliceable(asset):
    l, t, r, b = insets(asset)
    return l >= 0 and t >= 0 and r >= 0 and b >= 0 and l + r < asset["w"] and t + b < asset["h"]


def single_image(asset):
    name = asset["name"]
    if name.startswith(SINGLE_PREFIXES):
        return True
    # Sub-40 px ui_frames are single images by nature; the topbar plate is the
    # one 32 px-tall asset we deliberately 9-slice/tile.
    return (
        asset.get("atlas") == "ui_frames"
        and (asset["w"] < 40 or asset["h"] < 40)
        and not name.startswith("topbar_plate_")
    )


def sliced(asset):
    return sliceable(asset) and not single_image(asset) and any(value > 0 for value in insets(asset))


def summary(assets):
    items = list(assets)
    return {
        "assets": len(items),
        "sliced": sum(1 for asset in items if sliced(asset)),
        "unsliced": sum(1 for asset in items if not sliced(asset)),
        "insets_ignored": sum(1 for asset in items if not sliceable(asset)),
    }


def ssim_lite(a: np.ndarray, b: np.ndarray) -> float:
    a = a.astype(np.float64)
    b = b.astype(np.float64)
    mu_a = a.mean()
    mu_b = b.mean()
    var_a = a.var()
    var_b = b.var()
    cov = ((a - mu_a) * (b - mu_b)).mean()
    c1 = (0.01 * 255.0) ** 2
    c2 = (0.03 * 255.0) ** 2
    numerator = (2 * mu_a * mu_b + c1) * (2 * cov + c2)
    denominator = (mu_a * mu_a + mu_b * mu_b + c1) * (var_a + var_b + c2)
    return float(numerator / denominator) if denominator else 0.0


def inside(rect, outer, tol=0.75):
    x, y, w, h = rect
    ox, oy, ow, oh = outer
    return (
        x >= ox - tol
        and y >= oy - tol
        and x + w <= ox + ow + tol
        and y + h <= oy + oh + tol
    )


def check_one(manifest, layout, png_path, log_text, baseline, failures):
    root = Path(manifest["_path"]).parent
    assets = {asset["name"]: asset for asset in manifest["assets"]}
    expected_summary = (
        "assets={assets} sliced={sliced} unsliced={unsliced} "
        "insets_ignored={insets_ignored}"
    ).format(**summary(manifest["assets"]))
    if layout.get("summary") != expected_summary:
        failures.append(
            f"summary drift: layout={layout.get('summary')} expected={expected_summary}"
        )
    guard_failures = {name for name, asset in assets.items() if not sliceable(asset)}
    if guard_failures != KNOWN_DEFECTS:
        failures.append(
            "known-defect table drift: "
            f"manifest-only={sorted(guard_failures - KNOWN_DEFECTS)} "
            f"table-only={sorted(KNOWN_DEFECTS - guard_failures)}"
        )

    viewport = layout["viewport"]
    scale = float(layout["scale"])
    console = layout["console"]
    sections = layout["sections"]
    safe = layout["safe"]
    safe_frame = [
        safe[0],
        safe[1],
        viewport[0] - safe[0] - safe[2],
        viewport[1] - safe[1] - safe[3],
    ]
    topbar = next(
        (element["rect"] for element in layout["elements"] if element["id"] == "topbar"),
        [0, 0, viewport[0], 0],
    )

    # ---- sections ---------------------------------------------------------
    if layout["orientation"] == "Landscape":
        total = sum(section["rect"][2] for section in sections)
        if abs(total - viewport[0]) > 0.75:
            failures.append(f"sections width {total:.2f} != viewport {viewport[0]}")
    else:
        total = sum(section["rect"][3] for section in sections)
        if abs(total - console[3]) > 0.75:
            failures.append(f"sections height {total:.2f} != console {console[3]}")
    for section in sections:
        asset = assets.get(section["asset"])
        if asset is None:
            failures.append(f"section asset missing from manifest: {section['asset']}")
            continue
        if [round(value, 2) for value in (section.get("authored") or [])] != [asset["w"], asset["h"]]:
            failures.append(f"section {section['asset']} authored size drift")
        drawn = [draw for draw in layout["draws"] if draw["name"] == section["asset"]]
        if not drawn:
            failures.append(f"section {section['asset']} drew nothing")
        for draw in drawn:
            if not inside(draw["dst"], section["rect"], tol=0.75):
                failures.append(
                    f"section {section['asset']} quad leaves its section: {draw['dst']}"
                )
            u0, v0, u1, v1 = draw["uv"]
            if not (-1e-4 <= u0 <= 1.0001 and -1e-4 <= v0 <= 1.0001 and -1e-4 <= u1 <= 1.0001 and -1e-4 <= v1 <= 1.0001):
                failures.append(f"section {section['asset']} UV outside its source: {draw['uv']}")

    # ---- elements ---------------------------------------------------------
    for element in layout["elements"]:
        name = element["id"]
        rect = element["rect"]
        section = element.get("section", -1)
        art = element.get("art")
        if art:
            asset = assets.get(art)
            if asset is None:
                failures.append(f"{name}: art {art} not in manifest")
                continue
            if sliceable(asset) and not single_image(asset) and any(v > 0 for v in insets(asset)):
                corners = element.get("corners") or []
                if len(corners) != 4:
                    failures.append(f"{name}: sliced element ({art}) has no 4 corner quads")
                else:
                    l, t, r, b = insets(asset)
                    expected = [
                        (l, t, "TL"),
                        (r, t, "TR"),
                        (l, b, "BL"),
                        (r, b, "BR"),
                    ]
                    for corner, (want_w, want_h, label) in zip(corners, expected):
                        src = corner["src"]
                        if abs(src[2] - want_w) > 0.01 or abs(src[3] - want_h) > 0.01:
                            failures.append(
                                f"{name}: {label} corner src {src} != inset ({want_w},{want_h})"
                            )
                        dst = corner["dst"]
                        if abs(dst[2] - want_w * scale) > 0.8 or abs(dst[3] - want_h * scale) > 0.8:
                            failures.append(
                                f"{name}: {label} corner dst {dst} != inset*scale"
                            )
                        if not inside(dst, rect, tol=0.8):
                            failures.append(f"{name}: {label} corner leaves the element")
            else:
                stretch = element.get("stretch")
                if stretch and (stretch[0] > MAX_SINGLE_STRETCH + 1e-3 or stretch[1] > MAX_SINGLE_STRETCH + 1e-3):
                    failures.append(
                        f"{name}: single image {art} stretched {stretch[0]:.2f}x/{stretch[1]:.2f}x > {MAX_SINGLE_STRETCH}"
                    )
        if element.get("leaf"):
            if element.get("min_px", 0.0) + 0.01 < MIN_LEAF_PX:
                failures.append(
                    f"{name}: interactive leaf {element.get('min_px'):.1f} px < {MIN_LEAF_PX}"
                )
            if 0 <= section < len(sections):
                if not inside(rect, sections[section]["rect"], tol=0.75):
                    failures.append(
                        f"{name}: interactive element crosses its section boundary"
                    )
            else:
                failures.append(f"{name}: interactive element is outside the four sections")
        if 0 <= section < len(sections) and not inside(rect, sections[section]["rect"], tol=0.75):
            failures.append(f"{name}: element leaves its section")
        # viewport + safe area
        if not inside(rect, [0, 0, viewport[0], viewport[1]], tol=0.75):
            failures.append(f"{name}: outside the viewport")
        in_console = inside(rect, console, tol=0.75)
        in_topbar = inside(rect, topbar, tol=0.75)
        if element.get("leaf"):
            if in_console or in_topbar:
                if rect[0] < safe[0] - 0.75 or rect[0] + rect[2] > viewport[0] - safe[2] + 0.75:
                    failures.append(f"{name}: breaks the horizontal safe margin")
            elif not inside(rect, safe_frame, tol=0.75):
                failures.append(f"{name}: outside the safe area")

    # ---- texts ------------------------------------------------------------
    for text in layout["texts"]:
        if text["px"] + 0.01 < MIN_TEXT_PX:
            failures.append(f"text {text['id']}: {text['px']:.1f} px < {MIN_TEXT_PX}")
        rect = text["rect"]
        if not inside(rect, [0, 0, viewport[0], viewport[1]], tol=0.75):
            failures.append(f"text {text['id']}: outside the viewport")

    # ---- card inside the safe right edge ----------------------------------
    card = next((element for element in layout["elements"] if element["id"] == "card"), None)
    if card:
        right = card["rect"][0] + card["rect"][2]
        limit = viewport[0] - safe[2]
        if right > limit + 0.75:
            failures.append(f"card right {right:.1f} > safe edge {limit:.1f}")

    # ---- defect logs ------------------------------------------------------
    if log_text is not None:
        for name in sorted(KNOWN_DEFECTS):
            if not any(draw["name"] == name for draw in layout["draws"]):
                continue
            asset = assets[name]
            needle = f"HUD_PACK insets-ignored {asset['file']}"
            if needle not in log_text:
                failures.append(f"drawn defect {name}: missing log line '{needle}'")

    # ---- pixel / legacy look ---------------------------------------------
    image = Image.open(png_path).convert("RGB")
    if image.size != (int(viewport[0]), int(viewport[1])):
        failures.append(f"{png_path.name}: size {image.size} != viewport {viewport}")
        return
    x, y, w, h = (int(round(value)) for value in console)
    band = np.asarray(image.crop((max(0, x), max(0, y), x + w, y + h)))
    gray = band.mean(axis=2)
    unique = len(np.unique(band.reshape(-1, 3), axis=0))
    if unique < 2000:
        failures.append(f"console band has only {unique} colours: flat fill, not art")
    texture_score = 1.0 - ssim_lite(gray, np.full_like(gray, float(gray.mean())))
    if texture_score < 0.5:
        failures.append(f"console band texture score {texture_score:.3f} < 0.5")
    red = band[:, :, 0].astype(int)
    green = band[:, :, 1].astype(int)
    blue = band[:, :, 2].astype(int)
    magenta = float(np.mean((red > 190) & (blue > 190) & (green < 90)))
    if magenta > 0.002:
        failures.append(f"magenta wash {magenta * 100:.2f}% of the console band")
    stone = float(np.mean((np.abs(red - 58) < 4) & (np.abs(green - 52) < 4) & (np.abs(blue - 47) < 4)))
    if stone > 0.25:
        failures.append(f"legacy stone colour covers {stone * 100:.1f}% of the console band")
    print(
        f"{png_path.name}: colours={unique} texture_score={texture_score:.3f} magenta={magenta * 100:.3f}% stone={stone * 100:.2f}%"
    )

    if baseline is not None:
        other = np.asarray(Image.open(baseline).convert("L"))
        mine = np.asarray(image.convert("L"))
        if other.shape != mine.shape:
            failures.append(f"baseline size {other.shape} != {mine.shape}")
        else:
            score = ssim_lite(mine, other)
            print(f"ssim_lite({png_path.name},{baseline.name})={score:.4f}")
            if score < 0.90:
                failures.append(f"SSIM-lite vs baseline {score:.4f} < 0.90")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--layout", required=True, type=Path, action="append")
    parser.add_argument("--png", required=True, type=Path, action="append")
    parser.add_argument("--log", type=Path)
    parser.add_argument("--baseline", type=Path)
    args = parser.parse_args()
    if len(args.layout) != len(args.png):
        print("HUD_PACK_CHECK FAIL reason=layout/png count mismatch")
        return 2

    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    manifest["_path"] = str(args.manifest)
    root = args.manifest.parent
    failures: list[str] = []

    # completeness
    if len(manifest["assets"]) != 92:
        failures.append(f"asset count {len(manifest['assets'])} != 92")
    if len(manifest.get("font_files") or []) != 5:
        failures.append("font_files != 5")
    if len((manifest.get("atlas") or {}).get("commandbuttons", {}).get("entries") or []) != 20:
        failures.append("commandbuttons atlas entries != 20")
    for asset in manifest["assets"]:
        path = root / asset["file"]
        if not path.is_file():
            failures.append(f"missing asset file {asset['file']}")
            continue
        with Image.open(path) as image:
            if image.size != (asset["w"], asset["h"]):
                failures.append(
                    f"{asset['file']}: file {image.size} != manifest {(asset['w'], asset['h'])}"
                )
    for font in manifest.get("font_files") or []:
        if not (root / "fonts" / font).is_file():
            failures.append(f"missing font file fonts/{font}")

    log_text = args.log.read_text(encoding="utf-8", errors="replace") if args.log and args.log.is_file() else None
    for layout_path, png_path in zip(args.layout, args.png):
        layout = json.loads(layout_path.read_text(encoding="utf-8"))
        check_one(manifest, layout, png_path, log_text, args.baseline, failures)

    counts = summary(manifest["assets"])
    print(
        "assets={assets} sliced={sliced} unsliced={unsliced} insets_ignored={insets_ignored}".format(
            **counts
        )
    )
    if failures:
        for failure in failures:
            print(f"HUD_PACK_CHECK FAIL {failure}")
        print(f"HUD_PACK_CHECK FAIL count={len(failures)}")
        return 1
    print("HUD_PACK_CHECK PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
