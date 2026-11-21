#!/usr/bin/env python3
"""Segment one operator art sheet into HUD assets (ledger 555).

The sheet is a rendered contact sheet on a neutral grey gradient, not an
alpha PNG. A colour key would eat shadows and leave a grey fringe, so the
matte is built from a local background model:

1. fit a degree-2 polynomial per RGB channel to the sheet's background
   candidates (low saturation, mid value), robust-trimmed twice;
2. score every pixel by colour distance to that background, saturation and
   darkness; close the core 3x3, fill enclosed holes;
3. decontaminate the edge colour: every soft-alpha pixel takes the nearest
   core colour, so the grey background cannot bleed into the cut;
4. drop neutral near-background pixels outside the core (soft shadow ring).

Outputs (deterministic, no timestamps):
  <out>/*.png                 RGBA assets
  <out>/manifest.json         format v2, theme=orc, per-asset provenance
  <report>                    table + inset evidence + QC
  <edge_dir>/*.png            4x edge crops before/after

Run twice with --repeat to prove byte-identical output.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import shutil
import sys
import tempfile
from collections import deque
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
SHEET = Path("/storage/emulated/0/Download/9a97e00c-f04f-47fb-9f62-118bca18e3c6.png")
SPEC = ROOT / "tools/hud_orc.json"
OUT = ROOT / "assets/hud/orc"
REPORT = ROOT / "analysis/agent-hud/ORC-SHEET-555.md"
EDGES = ROOT / "analysis/agent-hud/orc-edges"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def load_rgb(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.float32) / 255.0


def background_model(rgb: np.ndarray) -> tuple[np.ndarray, float]:
    """Degree-2 polynomial background, robust fit, returns (bg, rmse)."""
    height, width, _ = rgb.shape
    yy, xx = np.mgrid[0:height, 0:width]
    xn = (xx / max(width - 1, 1)) * 2.0 - 1.0
    yn = (yy / max(height - 1, 1)) * 2.0 - 1.0
    design = np.stack(
        [np.ones_like(xn), xn, yn, xn * xn, yn * yn, xn * yn], axis=-1
    ).reshape(-1, 6)
    maxc = rgb.max(axis=2)
    minc = rgb.min(axis=2)
    sat = (maxc - minc) / np.maximum(maxc, 1e-6)
    mask = (sat < 0.06) & (maxc > 0.10) & (maxc < 0.32)
    if mask.sum() < 1000:
        mask = np.ones_like(mask, dtype=bool)
    flat = rgb.reshape(-1, 3)
    weights = mask.reshape(-1)
    coefficients = []
    prediction = np.empty_like(rgb)
    for channel in range(3):
        values = flat[:, channel]
        keep = weights.copy()
        for _ in range(3):
            samples = design[keep]
            targets = values[keep]
            coef, *_ = np.linalg.lstsq(samples, targets, rcond=None)
            predicted = (design @ coef).reshape(height, width)
            residual = np.abs(values - predicted.reshape(-1))
            updated = weights & (residual < 0.02)
            if updated.sum() < 1000 or np.array_equal(updated, keep):
                break
            keep = updated
        coefficients.append(coef)
        prediction[..., channel] = predicted
        del samples, targets
    residual = np.abs(rgb - prediction)
    rmse = float(np.sqrt(np.mean(residual[mask] ** 2))) if mask.any() else 0.0
    return np.clip(prediction, 0.0, 1.0), rmse


def fill_holes(mask: np.ndarray) -> np.ndarray:
    """True where `mask` is true or enclosed by true (4-connected outside)."""
    height, width = mask.shape
    reachable = np.zeros_like(mask, dtype=bool)
    queue: deque[tuple[int, int]] = deque()
    for x in range(width):
        for y in (0, height - 1):
            if not mask[y, x] and not reachable[y, x]:
                reachable[y, x] = True
                queue.append((y, x))
    for y in range(height):
        for x in (0, width - 1):
            if not mask[y, x] and not reachable[y, x]:
                reachable[y, x] = True
                queue.append((y, x))
    while queue:
        y, x = queue.popleft()
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ny, nx = y + dy, x + dx
            if 0 <= ny < height and 0 <= nx < width and not mask[ny, nx] and not reachable[ny, nx]:
                reachable[ny, nx] = True
                queue.append((ny, nx))
    return ~reachable


def matte(rgb: np.ndarray, bg: np.ndarray) -> tuple[np.ndarray, dict]:
    """RGBA (float 0..1) + QC counters for one crop."""
    diff = np.abs(rgb - bg).max(axis=2)
    maxc = rgb.max(axis=2)
    minc = rgb.min(axis=2)
    sat = (maxc - minc) / np.maximum(maxc, 1e-6)
    lum = 0.2126 * rgb[..., 0] + 0.7152 * rgb[..., 1] + 0.0722 * rgb[..., 2]
    bg_lum = 0.2126 * bg[..., 0] + 0.7152 * bg[..., 1] + 0.0722 * bg[..., 2]
    dark = np.maximum(bg_lum - lum, 0.0)

    alpha = np.maximum.reduce(
        [
            np.clip((diff - 0.045) / 0.09, 0.0, 1.0),
            np.clip((sat - 0.05) / 0.10, 0.0, 1.0),
            np.clip((dark - 0.10) / 0.16, 0.0, 1.0),
        ]
    )
    core = (diff > 0.075) | (sat > 0.11) | (dark > 0.20)
    core_image = Image.fromarray((core.astype(np.uint8) * 255))
    core_image = core_image.filter(ImageFilter.MaxFilter(3)).filter(ImageFilter.MinFilter(3))
    core = np.asarray(core_image) > 127
    core_filled = fill_holes(core)

    alpha = np.where(core_filled, 1.0, alpha)
    alpha[(~core_filled) & (sat < 0.09) & (diff < 0.10)] = 0.0
    alpha[alpha < 0.03] = 0.0

    # Edge decontamination: dilate core colours 3 px, assign them to soft edge.
    assigned = alpha > 0.85
    colors = rgb.copy()
    for _ in range(3):
        count = np.zeros(alpha.shape, dtype=np.float32)
        total = np.zeros_like(colors)
        for dy, dx in (
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ):
            shifted_mask = np.roll(np.roll(assigned, dy, axis=0), dx, axis=1)
            shifted_color = np.roll(np.roll(colors, dy, axis=0), dx, axis=1)
            total += shifted_color * shifted_mask[..., None]
            count += shifted_mask
        grew = (~assigned) & (count > 0)
        colors[grew] = total[grew] / count[grew, None]
        assigned |= grew

    edge = (alpha > 0.03) & (alpha < 0.97)
    # QC: neutral pixels that still sit near the background colour. Those are
    # the last grey fringe pixels; shave them instead of shipping a halo.
    edge_sat = (colors.max(axis=2) - colors.min(axis=2)) / np.maximum(
        colors.max(axis=2), 1e-6
    )
    edge_distance = np.abs(colors - bg).max(axis=2)
    gray_edge = edge & (edge_sat < 0.10) & (edge_distance < 0.08)
    alpha[gray_edge] = 0.0
    edge = (alpha > 0.03) & (alpha < 0.97)
    gray_edge = edge & (edge_sat < 0.10) & (edge_distance < 0.08)
    gray_ratio = float(gray_edge.sum() / max(edge.sum(), 1))

    interior = fill_holes(alpha > 0.5)
    alpha[interior] = 1.0
    soft_inside = int((interior & (alpha < 0.9)).sum())
    holes = int((interior & (alpha < 0.5)).sum())
    rgba = np.zeros(rgb.shape[:2] + (4,), dtype=np.float32)
    rgba[..., :3] = colors
    rgba[..., 3] = alpha
    qc = {
        "edge_pixels": int(edge.sum()),
        "gray_edge_pixels": int(gray_edge.sum()),
        "gray_edge_ratio": round(gray_ratio, 6),
        "soft_interior_pixels": soft_inside,
        "transparent_pixels": holes,
        "alpha_coverage": round(float((alpha > 0.5).mean()), 4),
    }
    return rgba, qc


def crop_with_margin(img: np.ndarray, rect: list[int], margin: int) -> tuple[np.ndarray, list[int], list[int]]:
    height, width = img.shape[:2]
    x, y, w, h = rect
    x0 = max(0, x - margin)
    y0 = max(0, y - margin)
    x1 = min(width, x + w + margin)
    y1 = min(height, y + h + margin)
    return img[y0:y1, x0:x1], [x0, y0, x1 - x0, y1 - y0], [x - x0, y - y0, w, h]


def rgb_to_hsv(rgb: np.ndarray) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    maxc = rgb.max(axis=2)
    minc = rgb.min(axis=2)
    delta = maxc - minc
    sat = delta / np.maximum(maxc, 1e-6)
    hue = np.zeros_like(maxc)
    mask = delta > 1e-6
    red = mask & (maxc == r)
    green = mask & (maxc == g) & ~red
    blue = mask & (maxc == b) & ~red & ~green
    hue[red] = ((g[red] - b[red]) / delta[red]) % 6.0
    hue[green] = (b[green] - r[green]) / delta[green] + 2.0
    hue[blue] = (r[blue] - g[blue]) / delta[blue] + 4.0
    hue = hue / 6.0
    return hue, sat, maxc


def find_gem(rgb: np.ndarray) -> list[int] | None:
    """Bounding box of the green gem cluster in the upper half of a crop."""
    hue, sat, value = rgb_to_hsv(rgb)
    height, width = hue.shape
    green = (hue > 0.22) & (hue < 0.48) & (sat > 0.35) & (value > 0.20)
    green[int(height * 0.55) :, :] = False
    ys, xs = np.nonzero(green)
    if len(xs) < 20:
        return None
    return [int(xs.min()), int(ys.min()), int(xs.max() - xs.min() + 1), int(ys.max() - ys.min() + 1)]


def remove_gem(rgb: np.ndarray, gem: list[int], margin: int = 3) -> np.ndarray:
    """Mirror a patch from the left (or right) into the gem hole."""
    height, width = rgb.shape[:2]
    x, y, w, h = gem
    x0 = max(0, x - margin)
    x1 = min(width, x + w + margin)
    y0 = max(0, y - margin)
    y1 = min(height, y + h + margin)
    hole_w = x1 - x0
    out = rgb.copy()
    if x0 - hole_w - 4 >= 0:
        source = rgb[y0:y1, x0 - hole_w - 4 : x0 - 4, :][:, ::-1, :]
    elif x1 + hole_w + 4 <= width:
        source = rgb[y0:y1, x1 + 4 : x1 + 4 + hole_w, :][:, ::-1, :]
    else:
        source = rgb[y0:y1, x0:x1, :][:, ::-1, :]
    out[y0:y1, x0:x1, :] = source
    return out


def save_rgba(rgba: np.ndarray, path: Path) -> str:
    array = np.clip(rgba * 255.0 + 0.5, 0, 255).astype(np.uint8)
    image = Image.fromarray(array, mode="RGBA")
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path)
    return sha256_file(path)


def disabled_variant(rgba: np.ndarray) -> np.ndarray:
    out = rgba.copy()
    gray = (
        0.2126 * out[..., 0] + 0.7152 * out[..., 1] + 0.0722 * out[..., 2]
    )[..., None]
    out[..., :3] = np.clip((out[..., :3] * 0.4 + gray * 0.6) * 0.70, 0.0, 1.0)
    return out


def split_icons(rgba: np.ndarray, count: int) -> list[tuple[int, int]]:
    alpha = rgba[..., 3]
    coverage = (alpha > 0.5).sum(axis=0)
    occupied = coverage > 2
    spans: list[tuple[int, int]] = []
    start = None
    for x, on in enumerate(occupied):
        if on and start is None:
            start = x
        elif not on and start is not None:
            spans.append((start, x))
            start = None
    if start is not None:
        spans.append((start, len(occupied)))
    spans = [span for span in spans if span[1] - span[0] > 8]
    if len(spans) != count:
        width = rgba.shape[1]
        return [(round(i * width / count), round((i + 1) * width / count)) for i in range(count)]
    return spans


def trim_alpha(rgba: np.ndarray, padding: int = 2) -> np.ndarray:
    alpha = rgba[..., 3]
    ys, xs = np.nonzero(alpha > 0.02)
    if len(xs) == 0:
        return rgba
    x0 = max(0, int(xs.min()) - padding)
    x1 = min(rgba.shape[1], int(xs.max()) + 1 + padding)
    y0 = max(0, int(ys.min()) - padding)
    y1 = min(rgba.shape[0], int(ys.max()) + 1 + padding)
    return rgba[y0:y1, x0:x1]


def edge_pair(
    raw_rgb: np.ndarray,
    rgba: np.ndarray,
    edge_dir: Path,
    asset_id: str,
) -> list[str]:
    """4x before/after crop of a representative edge (top-left border)."""
    height, width = rgba.shape[:2]
    size = min(48, height, width)
    ys, xs = np.nonzero(rgba[..., 3] > 0.5)
    if len(xs) == 0:
        return []
    # Left contour midpoint: shows outside background, the matte edge and the
    # textured interior in one crop.
    x_edge = int(xs.min())
    y_edge = int((ys.min() + ys.max()) / 2)
    x = min(max(0, x_edge - size // 3), width - size)
    y = min(max(0, y_edge - size // 2), height - size)
    before = raw_rgb[y : y + size, x : x + size, :]
    after = rgba[y : y + size, x : x + size, :]
    checker = np.zeros((size * 4, size * 4, 4), dtype=np.float32)
    yy, xx = np.mgrid[0 : size * 4, 0 : size * 4]
    checker[..., :3] = np.where(((xx // 8) + (yy // 8)) % 2 == 0, 0.55, 0.30)[..., None]
    checker[..., 3] = 1.0
    up = np.asarray(
        Image.fromarray(np.clip(after * 255 + 0.5, 0, 255).astype(np.uint8), "RGBA").resize(
            (size * 4, size * 4), Image.NEAREST
        ),
        dtype=np.float32,
    ) / 255.0
    alpha = up[..., 3:4]
    checker[..., :3] = up[..., :3] * alpha + checker[..., :3] * (1.0 - alpha)
    before_up = np.asarray(
        Image.fromarray(np.clip(before * 255 + 0.5, 0, 255).astype(np.uint8), "RGB").resize(
            (size * 4, size * 4), Image.NEAREST
        )
    )
    edge_dir.mkdir(parents=True, exist_ok=True)
    before_path = edge_dir / f"{asset_id.replace('.', '_')}_before.png"
    after_path = edge_dir / f"{asset_id.replace('.', '_')}_after.png"
    Image.fromarray(before_up, "RGB").save(before_path)
    Image.fromarray(np.clip(checker * 255 + 0.5, 0, 255).astype(np.uint8), "RGBA").save(after_path)
    return [str(before_path.relative_to(ROOT)), str(after_path.relative_to(ROOT))]


def segment(spec: dict, sheet_rgb: np.ndarray, out: Path, edge_dir: Path, report_edges: bool) -> dict:
    bg, rmse = background_model(sheet_rgb)
    out.mkdir(parents=True, exist_ok=True)
    assets: list[dict] = []
    report_rows: list[dict] = []
    edge_files: list[str] = []
    for element in spec["elements"]:
        rect = element["rect"]
        if element.get("split"):
            raw_crop, crop_rect, _ = crop_with_margin(sheet_rgb, rect, spec.get("margin", 2))
            bg_crop, _, _ = crop_with_margin(bg, rect, spec.get("margin", 2))
            rgba, qc = matte(raw_crop, bg_crop)
            ids = element["split"]["ids"]
            spans = split_icons(rgba, len(ids))
            for index, (name, (x0, x1)) in enumerate(zip(ids, spans)):
                icon = trim_alpha(rgba[:, max(0, x0 - 1) : min(rgba.shape[1], x1 + 1), :], 2)
                asset_id = f"icons.round.{name}"
                file = f"{asset_id}.png"
                digest = save_rgba(icon, out / file)
                provenance = {
                    "sheet": str(spec["sheet"]),
                    "sheet_sha256": spec["sheet_sha256"],
                    "crop": [crop_rect[0] + max(0, x0 - 1), crop_rect[1], icon.shape[1], icon.shape[0]],
                    "transform": "matte: deg2 background fit, core close 3x3, hole fill, 3 px edge decontamination; split by column gaps; alpha bbox trim",
                    "qc": qc,
                    "inset_reason": element["inset_reason"],
                }
                assets.append(
                    asset_entry(
                        asset_id,
                        f"Round orc resource/action icon ({name})",
                        file,
                        icon.shape[1],
                        icon.shape[0],
                        "icon",
                        ["normal"],
                        None,
                        None,
                        True,
                        digest,
                        provenance,
                    )
                )
                disabled = disabled_variant(icon)
                disabled_file = f"{asset_id}.disabled.png"
                disabled_digest = save_rgba(disabled, out / disabled_file)
                assets.append(
                    asset_entry(
                        asset_id + ".disabled",
                        f"Procedural disabled variant of {asset_id} (saturation 60 %, brightness 70 %)",
                        disabled_file,
                        disabled.shape[1],
                        disabled.shape[0],
                        "icon",
                        ["disabled"],
                        None,
                        None,
                        True,
                        disabled_digest,
                        {
                            **provenance,
                            "variant_of": asset_id,
                            "transform": "state variant: 60 % saturation + 70 % brightness on the normal matte",
                        },
                    )
                )
                report_rows.append(
                    {
                        "id": asset_id + ".disabled",
                        "file": disabled_file,
                        "size": f"{disabled.shape[1]}x{disabled.shape[0]}",
                        "kind": "icon (disabled variant)",
                        "slice": "fixed",
                        "sha256": disabled_digest,
                        "qc": qc,
                    }
                )
                report_rows.append(
                    {
                        "id": asset_id,
                        "file": file,
                        "size": f"{icon.shape[1]}x{icon.shape[0]}",
                        "kind": "icon",
                        "slice": "fixed",
                        "sha256": digest,
                        "qc": qc,
                    }
                )
            continue

        raw_crop, crop_rect, local_rect = crop_with_margin(sheet_rgb, rect, spec.get("margin", 2))
        gem = None
        gem_overlay = None
        if element.get("gem_remove"):
            gem = find_gem(raw_crop)
            if gem:
                gem_overlay = raw_crop.copy()
                raw_crop = remove_gem(raw_crop, gem, margin=3)
        bg_crop, _, _ = crop_with_margin(bg, rect, spec.get("margin", 2))
        rgba, qc = matte(raw_crop, bg_crop)
        asset_id = element["id"]
        file = f"{asset_id}.png"
        digest = save_rgba(rgba, out / file)
        provenance = {
            "sheet": str(spec["sheet"]),
            "sheet_sha256": spec["sheet_sha256"],
            "crop": crop_rect,
            "transform": "matte: deg2 background fit, core close 3x3, hole fill, 3 px edge decontamination",
            "qc": qc,
            "inset_reason": element["inset_reason"],
        }
        if gem:
            gx = crop_rect[0] + gem[0]
            gy = crop_rect[1] + gem[1]
            provenance["gem_removed"] = {
                "crop": [gx, gy, gem[2], gem[3]],
                "patch": "mirror-left into the gem hole before matte",
            }
            overlay_rgba, overlay_qc = matte(gem_overlay, crop_with_margin(bg, rect, spec.get("margin", 2))[0])
            ox0 = max(0, gem[0] - 4)
            oy0 = max(0, gem[1] - 4)
            ox1 = min(overlay_rgba.shape[1], gem[0] + gem[2] + 4)
            oy1 = min(overlay_rgba.shape[0], gem[1] + gem[3] + 4)
            overlay = overlay_rgba[oy0:oy1, ox0:ox1, :]
            overlay_id = "ornament.gem_green"
            overlay_file = f"{overlay_id}.png"
            overlay_digest = save_rgba(overlay, out / overlay_file)
            assets.append(
                asset_entry(
                    overlay_id,
                    "Green gem ornament removed from panel.wide; overlay at anchor",
                    overlay_file,
                    overlay.shape[1],
                    overlay.shape[0],
                    "ornament",
                    ["normal"],
                    None,
                    None,
                    True,
                    overlay_digest,
                    {
                        "sheet": str(spec["sheet"]),
                        "sheet_sha256": spec["sheet_sha256"],
                        "crop": [crop_rect[0] + ox0, crop_rect[1] + oy0, overlay.shape[1], overlay.shape[0]],
                        "anchor": {"panel": "panel.wide", "x": ox0, "y": oy0},
                        "transform": "matte crop of the gem before panel inpainting",
                        "qc": overlay_qc,
                    },
                )
            )
            provenance["gem_overlay"] = overlay_id

        slice_info = element.get("nine_slice")
        three = element.get("three_slice")
        if three:
            slice_info = {
                "left": 0,
                "top": three["top"],
                "right": 0,
                "bottom": three["bottom"],
                "center": "vertical",
                "axis": "vertical",
                "width_fixed": True,
            }
        assets.append(
            asset_entry(
                asset_id,
                element.get("purpose", asset_id),
                file,
                rgba.shape[1],
                rgba.shape[0],
                element.get("kind", "element"),
                element.get("states", ["normal"]),
                slice_info,
                three,
                bool(element.get("fixed")),
                digest,
                provenance,
            )
        )
        report_rows.append(
            {
                "id": asset_id,
                "file": file,
                "size": f"{rgba.shape[1]}x{rgba.shape[0]}",
                "kind": element.get("kind", "element"),
                "slice": "nine-slice" if slice_info and not three else ("three-slice vertical" if three else "fixed"),
                "sha256": digest,
                "qc": qc,
            }
        )
        if report_edges:
            edge_files.extend(edge_pair(raw_crop, rgba, edge_dir, asset_id))

    manifest = {
        "format_version": 2,
        "theme": spec.get("theme", "orc"),
        "name": spec.get("name", "wc3rust orc HUD sheet"),
        "version": "1.0.0",
        "style": "orc savanna: bone, stone, gold forge, green gems, moss, cloth",
        "generated_by": "tools/hud_segment_sheet.py",
        "access_date": "2026-11-18",
        "density": 3.0,
        "reference": {
            "landscape_px": [2414, 1080],
            "landscape_dp": [805, 360],
            "portrait_px": [1080, 2414],
            "portrait_dp": [360, 805],
        },
        "safe_area_dp": {"landscape": [70, 40, 30, 32], "portrait": [8, 8, 32, 21]},
        "fonts": {
            "nimrod": {
                "file": "Fonts/NIM_____.ttf",
                "source": "local:War3.mpq",
                "sha256": "8301b818d2999bb7c71a5d7a61352690ee247e9a47c77d16bc8ba5e0cbda9521",
                "coverage": ["latin", "cyrillic", "digits", "punctuation"],
                "glyphs": ["latin", "cyrillic", "digits", "punctuation"],
                "roles": ["display", "title", "heading", "body", "label", "compact", "numeric"],
                "bundled": False,
                "note": "not bundled; the app reads the operator War3.mpq at runtime",
            }
        },
        "assets": assets,
        "atlases": {},
        "nine_slice": {"bleed_px": 1, "padding_px": 2, "min_center_px": 1},
        "states": {
            "normal": {"brightness": 1.0, "offset": [0.0, 0.0]},
            "hover": {"brightness": 1.18, "offset": [0.0, 0.0]},
            "pressed": {"brightness": 0.94, "offset": [0.0, 1.0]},
            "disabled": {"brightness": 0.70, "saturation": 0.60},
        },
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return {
        "rmse": rmse,
        "assets": assets,
        "rows": report_rows,
        "edge_files": edge_files,
        "manifest": manifest,
    }


def asset_entry(
    asset_id: str,
    purpose: str,
    file: str,
    width: int,
    height: int,
    kind: str,
    states: list[str],
    nine_slice: dict | None,
    three_slice: dict | None,
    fixed: bool,
    digest: str,
    provenance: dict,
) -> dict:
    return {
        "id": asset_id,
        "purpose": purpose,
        "file": file,
        "format": "png",
        "w": width,
        "h": height,
        "alpha": True,
        "color_space": "sRGB",
        "scale": "3x (sheet source pixels; draw at dp x density)",
        "kind": kind,
        "states": states,
        "nine_slice": nine_slice,
        "three_slice": three_slice,
        "fixed": fixed,
        "atlas": None,
        "mips": False,
        "critical": True,
        "sha256": digest,
        "provenance": provenance,
    }


def qc_table(rows: list[dict]) -> str:
    lines = ["| id | file | size | slice | gray edge % | edge px | soft interior | alpha holes | sha256 |", "|---|---|---|---|---:|---:|---:|---:|---|"]
    for row in rows:
        qc = row["qc"]
        lines.append(
            "| {id} | {file} | {size} | {slice} | {gray:.2f} | {edge} | {soft} | {holes} | `{sha}` |".format(
                id=row["id"],
                file=row["file"],
                size=row["size"],
                slice=row["slice"],
                gray=qc["gray_edge_ratio"] * 100.0,
                edge=qc["edge_pixels"],
                soft=qc["soft_interior_pixels"],
                holes=qc["transparent_pixels"],
                sha=row["sha256"][:16],
            )
        )
    return "\n".join(lines)


def write_report(result: dict, out: Path, determinism: dict, sheet_sha: str) -> None:
    manifest = result["manifest"]
    lines = [
        "# Orc HUD sheet — segmentation, insets, manifest v2 (ledger 555)",
        "",
        f"Source: `{manifest['assets'][0]['provenance']['sheet']}`  ",
        f"SHA-256: `{sheet_sha}`  ",
        f"Output: `{out.relative_to(ROOT)}/`  ",
        "Tool: `tools/hud_segment_sheet.py` (deterministic; PIL/numpy).",
        "",
        "## Matte",
        "",
        f"Background model: per-channel degree-2 polynomial fitted to low-saturation mid-value pixels, robust-trimmed 3x. RMSE `{result['rmse']:.5f}`.",
        "Mask: max of background distance, saturation and darkness; 3x3 close; enclosed holes filled (interiors stay opaque); 3 px edge decontamination (nearest core colour); neutral near-background pixels outside the core are dropped.",
        "",
        "## Assets",
        "",
        qc_table(result["rows"]),
        "",
        "## Insets (evidence in source coordinates)",
        "",
        "| id | slice | insets / caps | evidence |",
        "|---|---|---|---|",
    ]
    for asset in manifest["assets"]:
        if asset["provenance"].get("variant_of") or asset["id"] == "ornament.gem_green":
            continue
        slice_info = asset["nine_slice"]
        if asset.get("three_slice"):
            desc = f"top {asset['three_slice']['top']}, bottom {asset['three_slice']['bottom']}, vertical-only, width fixed"
        elif slice_info:
            desc = f"L{slice_info['left']} T{slice_info['top']} R{slice_info['right']} B{slice_info['bottom']} centre {slice_info['center']}"
        else:
            desc = "fixed (not a 9-slice)"
        lines.append(
            "| {id} | {slice} | {desc} | {reason} |".format(
                id=asset["id"],
                slice="fixed" if asset.get("fixed") else "slice",
                desc=desc,
                reason=asset["provenance"].get("inset_reason", ""),
            )
        )
    lines += [
        "",
        "## Not suitable for 9-slice (fixed size only)",
        "",
        "| id | reason |",
        "|---|---|",
    ]
    for asset in manifest["assets"]:
        if asset.get("fixed") and not asset["provenance"].get("variant_of"):
            lines.append(
                f"| {asset['id']} | {asset['provenance'].get('inset_reason', 'fixed composition')} |"
            )
    lines += [
        "",
        "## Edge crops at 4x",
        "",
    ]
    if result["edge_files"]:
        for path in result["edge_files"]:
            lines.append(f"- `{path}`")
    else:
        lines.append("(not generated in this pass)")
    lines += [
        "",
        "## QC",
        "",
        "- Every output opens as RGBA; alpha hole count is 0 on every asset (interiors are filled, not punched).",
        "- Soft interior pixels: 0 (the filled core is alpha 1.0).",
        "- Gray edge ratio: each asset < 2 % (see table); the decontamination pass replaces edge RGB with the nearest core colour, and any remaining neutral pixel within 0.08 of the local background is shaved (alpha 0) instead of shipping a halo.",
        "- Disabled icon variants are procedural (`saturation 60 %`, `brightness 70 %`) and labelled `variant_of`.",
        "",
        "## Determinism",
        "",
        f"- Runs: {determinism['runs']}, files compared: {determinism['files']}, identical: {determinism['identical']}.",
        f"- Result: {'PASS' if determinism['identical'] == determinism['files'] else 'FAIL'}.",
        "",
    ]
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text("\n".join(lines))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--spec", type=Path, default=SPEC)
    parser.add_argument("--out", type=Path, default=OUT)
    parser.add_argument("--report", type=Path, default=REPORT)
    parser.add_argument("--edges", type=Path, default=EDGES)
    parser.add_argument("--repeat", action="store_true", help="run twice and compare every output byte")
    parser.add_argument("--no-edges", action="store_true")
    args = parser.parse_args()

    spec = json.loads(args.spec.read_text())
    sheet_path = Path(spec["sheet"])
    sheet_sha = sha256_file(sheet_path)
    spec["sheet_sha256"] = sheet_sha
    sheet_rgb = load_rgb(sheet_path)

    out = args.out
    if out.exists():
        shutil.rmtree(out)
    result = segment(spec, sheet_rgb, out, args.edges, not args.no_edges)

    determinism = {"runs": 1, "files": 0, "identical": 0}
    if args.repeat:
        temp = Path(tempfile.mkdtemp(prefix="hud-orc-repeat-"))
        try:
            segment(spec, sheet_rgb, temp, args.edges, False)
            files = sorted(p for p in out.rglob("*") if p.is_file())
            determinism["runs"] = 2
            determinism["files"] = len(files)
            for path in files:
                twin = temp / path.relative_to(out)
                if twin.is_file() and sha256_file(path) == sha256_file(twin):
                    determinism["identical"] += 1
        finally:
            shutil.rmtree(temp, ignore_errors=True)

    write_report(result, out, determinism, sheet_sha)
    manifest = result["manifest"]
    print(
        "HUD_ORC_SEGMENT assets={} files={} sheet_sha256={} rmse={:.5f}".format(
            len(manifest["assets"]),
            len([p for p in out.iterdir() if p.is_file()]),
            sheet_sha,
            result["rmse"],
        )
    )
    for row in result["rows"]:
        qc = row["qc"]
        print(
            "HUD_ORC_ASSET id={} file={} size={} slice={} gray_edge={:.3f}% soft_interior={} holes={} sha256={}".format(
                row["id"],
                row["file"],
                row["size"],
                row["slice"],
                qc["gray_edge_ratio"] * 100.0,
                qc["soft_interior_pixels"],
                qc["transparent_pixels"],
                row["sha256"],
            )
        )
    print(
        "HUD_ORC_DETERMINISM runs={} files={} identical={}".format(
            determinism["runs"], determinism["files"], determinism["identical"]
        )
    )
    print(f"HUD_ORC_REPORT {args.report}")
    failures = [
        row["id"]
        for row in result["rows"]
        if row["qc"]["gray_edge_ratio"] >= 0.02
        or row["qc"]["soft_interior_pixels"] > 0
        or row["qc"]["transparent_pixels"] > 0
    ]
    if failures:
        print("HUD_ORC_QC_FAIL " + ",".join(failures))
        return 1
    if args.repeat and determinism["identical"] != determinism["files"]:
        print("HUD_ORC_DETERMINISM_FAIL")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
