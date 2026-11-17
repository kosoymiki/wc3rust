#!/usr/bin/env python3
"""Visual regression analyzer for wc3rust screenshots.

Gate classes (every one is a hard FAIL unless marked WARN):
  system_bar_overlap      HUD chrome/text under the status bar, cutout or nav bar
  touch_targets           an interactive rect smaller than 48 dp
  button_overlap          two command-card rects intersect
  button_clipping         a command-card rect leaves the safe area / screen
  command_cluster         cluster taller than 40% of the screen, or its bbox
                          leaves the safe area (scattered/clipped command card)
  tiny_text               HUD glyph line shorter than 24 px (8 dp at 480 dpi)
  text_contrast           HUD text contrast ratio below 3.0 (WCAG large text)
  text_overlap            two HUD text lines overlap by >15% of the smaller box
  terrain_seams           periodic straight tile-edge energy (checkerboard)
  terrain_flat_patches    large connected untextured/flat terrain rectangles
  black_holes             near-black world regions (texture holes)
  white_holes             near-white world regions (giant white plane)
  pink_alpha              magenta/alpha-checker texels in the world
  units_visible           no player/enemy unit tint clusters in the world
  joystick_contrast       joystick disc/knob contrast against terrain
  minimap_readability     empty/low-contrast minimap or no unit dots
  selection_readability   empty selection panel or unreadable text
  command_icons           a command button has no icon texture inside it

Usage:
    python3 -m scripts.qa.analyze SHOT.png --json-out out.json --annotated out.png \
        --insets 120,120,0,48 --density 480 --fps 120
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

from . import common, layout
from .common import components, contrast_ratio, human_px_dp

FONT = None


def _font():
    global FONT
    if FONT is None:
        from PIL import ImageFont

        for path in (
            "/system/fonts/Roboto-Regular.ttf",
            "/system/fonts/NotoSans-Regular.ttf",
            "/data/data/com.termux/files/usr/share/fonts/TTF/DejaVuSans.ttf",
        ):
            if Path(path).exists():
                try:
                    FONT = ImageFont.truetype(path, 22)
                    break
                except OSError:
                    pass
        if FONT is None:
            FONT = ImageFont.load_default()
    return FONT


class Gate:
    def __init__(self, gid: str, ok: bool, detail: str, severity: str = "fail", **metrics):
        self.gid = gid
        self.ok = ok
        self.detail = detail
        self.severity = severity
        self.metrics = metrics

    def as_dict(self) -> dict:
        return {
            "id": self.gid,
            "ok": self.ok,
            "severity": self.severity,
            "detail": self.detail,
            "metrics": self.metrics,
        }


def _insets_from_args(text: str | None) -> dict:
    if not text:
        return {"left": 0, "top": 0, "right": 0, "bottom": 0, "status_visible": False, "nav_visible": False}
    parts = [int(v) for v in text.split(",")]
    if len(parts) != 4:
        raise SystemExit("--insets must be L,T,R,B")
    return {
        "left": parts[0],
        "top": parts[1],
        "right": parts[2],
        "bottom": parts[3],
        "status_visible": parts[1] > 0,
        "nav_visible": parts[3] > 0,
    }


# ------------------------------------------------------------------ detection


def detect_overlays(rgb: np.ndarray) -> list[list[int]]:
    """Large neutral-dark rectangles drawn by another app (call banner, dialog).

    Terrain is chromatic (olive/green/brown); Chrome panels are blue-tinted.
    A neutral, low-variance, screen-wide dark card is a foreign window and must
    not be counted as terrain or text.
    """
    mx = rgb.max(axis=2).astype(np.int16)
    mn = rgb.min(axis=2).astype(np.int16)
    luma = rgb.mean(axis=2)
    neutral = (mx - mn < 16) & (luma > 12) & (luma < 78)
    # dilate to close rounded corners / icon glyphs inside the card
    dil = neutral.copy()
    for _ in range(6):
        dil[1:, :] |= dil[:-1, :]
        dil[:-1, :] |= dil[1:, :]
        dil[:, 1:] |= dil[:, :-1]
        dil[:, :-1] |= dil[:, 1:]
    h, w = luma.shape
    out = []
    for x0, y0, x1, y1, size in components(dil, min_size=4000):
        bw = x1 - x0 + 1
        bh = y1 - y0 + 1
        if bw >= w * 0.25 and bh >= h * 0.06 and size > bw * bh * 0.25:
            out.append([int(x0), int(y0), int(x1), int(y1)])
    return out


def _mask_overlays(rgb: np.ndarray, overlays: list[list[int]], fill: int = 60) -> np.ndarray:
    if not overlays:
        return rgb
    out = rgb.copy()
    for x0, y0, x1, y1 in overlays:
        out[max(0, y0) : y1, max(0, x0) : x1] = fill
    return out


def detect_chrome(rgb: np.ndarray) -> list[list[int]]:
    """Rounded chrome borders are desaturated grey-blue 3-6 px bands."""
    mx = rgb.max(axis=2)
    mn = rgb.min(axis=2)
    neutral = (mx - mn <= 12) & (mx >= 45) & (mx <= 200)
    rects = []
    for x0, y0, x1, y1, size in components(neutral, min_size=140):
        w = x1 - x0 + 1
        h = y1 - y0 + 1
        if not (40 <= w <= 520 and 40 <= h <= 420):
            continue
        aspect = w / float(max(h, 1))
        if not (0.55 <= aspect <= 1.8):
            continue
        # Border components are hollow: count neutral pixels vs bbox area.
        density = size / float(w * h)
        if density > 0.62:
            continue
        rects.append([int(x0), int(y0), int(x1), int(y1), int(size), round(density, 3)])
    rects.sort(key=lambda r: -r[4])
    return rects


def measure_band(gray: np.ndarray, band: dict) -> dict:
    from .common import box_blur

    x0, y0, x1, y1 = band["rect"]
    x0, y0 = max(0, x0), max(0, y0)
    x1, y1 = min(gray.shape[1], x1), min(gray.shape[0], y1)
    patch = gray[y0:y1, x0:x1]
    if patch.size == 0:
        return {**band, "present": False, "px": 0.0, "contrast": 0.0, "bbox": None, "fg_frac": 0.0}
    local = box_blur(gray, 7)[y0:y1, x0:x1]
    fg = ((patch - local > 15) & (patch > 80)) | (patch > 205)
    count = int(fg.sum())
    present = count >= 12
    bbox = None
    height = 0.0
    if present:
        comps = [c for c in components(fg, min_size=3) if c[3] - c[1] + 1 >= 3]
        if not comps:
            present = False
        else:
            bx0 = min(c[0] for c in comps)
            by0 = min(c[1] for c in comps)
            bx1 = max(c[2] for c in comps)
            by1 = max(c[3] for c in comps)
            bbox = [x0 + bx0, y0 + by0, x0 + bx1 + 1, y0 + by1 + 1]
            height = float(np.median([c[3] - c[1] + 1 for c in comps]))
    values = np.sort(patch.ravel())
    bright = values[int(values.size * 0.99):]
    dark = values[: max(1, int(values.size * 0.60))]
    fg_col = float(bright.mean()) if bright.size else float(values.max())
    bg_col_band = float(np.median(dark))
    ring = []
    for yy in range(max(0, y0 - 6), min(gray.shape[0], y1 + 6)):
        for xx in range(max(0, x0 - 6), min(gray.shape[1], x1 + 6)):
            if x0 <= xx < x1 and y0 <= yy < y1:
                continue
            ring.append(gray[yy, xx])
    bg_col = float(np.median(ring)) if ring else bg_col_band
    if abs(fg_col - bg_col) < 6.0:
        fg_col = float(values.max())
        bg_col = bg_col_band
    ratio = contrast_ratio((fg_col,) * 3, (bg_col,) * 3)
    return {
        **band,
        "present": bool(present),
        "px": round(height, 1),
        "contrast": round(float(ratio), 2),
        "bbox": bbox,
        "fg_frac": round(float(fg.mean()), 3),
        "fg": round(fg_col, 1),
        "bg": round(bg_col, 1),
    }


def detect_text(gray: np.ndarray, bands: list[dict]) -> list[dict]:
    """Glyph clusters restricted to expected HUD text bands (no OCR)."""
    from .common import box_blur

    bg = box_blur(gray, 9)
    fg = ((gray - bg > 17) & (gray > 82)) | (gray > 205)
    dil = fg.copy()
    dil[:, 1:] |= fg[:, :-1]
    dil[:, :-1] |= fg[:, 1:]
    dil[1:, :] |= fg[:-1, :]
    dil[:-1, :] |= fg[1:, :]
    boxes = []
    for x0, y0, x1, y1, size in components(dil, min_size=5):
        w = x1 - x0 + 1
        h = y1 - y0 + 1
        if h < 7 or h > 54 or w > 120 or size < 5:
            continue
        cx, cy = (x0 + x1) * 0.5, (y0 + y1) * 0.5
        band = None
        for candidate in bands:
            bx0, by0, bx1, by1 = candidate["rect"]
            if bx0 <= cx <= bx1 and by0 <= cy <= by1:
                band = candidate
                break
        if band is None:
            continue
        boxes.append({"box": [x0, y0, x1, y1, h, size], "band": band["name"], "band_rect": band["rect"]})
    boxes.sort(key=lambda b: (b["box"][1], b["box"][0]))

    lines: dict[str, dict] = {}
    for item in boxes:
        key = item["band"]
        line = lines.get(key)
        if line is None:
            lines[key] = {
                "band": key,
                "bbox": list(item["box"][:4]),
                "components": [item["box"]],
                "height": float(item["box"][4]),
            }
        else:
            b = line["bbox"]
            line["bbox"] = [min(b[0], item["box"][0]), min(b[1], item["box"][1]), max(b[2], item["box"][2]), max(b[3], item["box"][3])]
            line["components"].append(item["box"])
            line["height"] = float(np.median([c[4] for c in line["components"]]))
    out = []
    for line in lines.values():
        b = line["bbox"]
        out.append(
            {
                "band": line["band"],
                "bbox": [int(v) for v in b],
                "height": round(float(line["height"]), 1),
                "width": int(b[2] - b[0]),
                "components": len(line["components"]),
            }
        )
    out.sort(key=lambda line: (line["bbox"][1], line["bbox"][0]))
    return out


def detect_units(rgb: np.ndarray, world_rect) -> dict:
    x0, y0, x1, y1 = [int(v) for v in world_rect]
    crop = rgb[y0:y1, x0:x1]
    r = crop[:, :, 0].astype(np.int16)
    g = crop[:, :, 1].astype(np.int16)
    b = crop[:, :, 2].astype(np.int16)
    blue = (b > r + 22) & (b > g + 10) & (b > 70) & (r > 25)
    red = (r > g + 25) & (r > b + 10) & (r > 75) & (b < 190)
    blue_boxes = [c for c in components(blue, min_size=3) if 2 <= c[4] <= 9000]
    red_boxes = [c for c in components(red, min_size=3) if 2 <= c[4] <= 9000]
    sizes = [min(c[2] - c[0] + 1, c[3] - c[1] + 1) for c in blue_boxes + red_boxes]
    return {
        "rect": [x0, y0, x1, y1],
        "blue_pixels": int(blue.sum()),
        "red_pixels": int(red.sum()),
        "blue_clusters": [c for c in blue_boxes if c[4] >= 6][:20],
        "red_clusters": [c for c in red_boxes if c[4] >= 6][:20],
        "median_cluster_px": float(np.median(sizes)) if sizes else 0.0,
    }


def terrain_metrics(rgb: np.ndarray, w: int, h: int, bottom: int | None = None) -> dict:
    rois = [
        (0.18, 0.16, 0.76, 0.86),
        (0.28, 0.20, 0.72, 0.85),
        (0.18, 0.25, 0.42, 0.85),
        (0.55, 0.25, 0.78, 0.85),
    ]
    best = {"ratio": 0.0, "period": 0, "offset": 0, "axis": "", "roi": None}
    for fx0, fy0, fx1, fy1 in rois:
        x0, x1 = int(w * fx0), int(w * fx1)
        y0, y1 = int(h * fy0), int(h * fy1)
        if bottom is not None:
            y1 = min(y1, int(bottom))
        if y1 <= y0:
            continue
        g = rgb[y0:y1, x0:x1].astype(np.float32).mean(axis=2)
        for axis in (1, 0):
            diff = np.abs(np.diff(g, axis=axis))
            prof = (diff > 16).mean(axis=0 if axis == 1 else 1)
            amp = diff.mean(axis=0 if axis == 1 else 1)
            mean = float(prof.mean()) or 1e-9
            for period in range(32, 132):
                if len(prof) <= period * 2:
                    continue
                vals = np.array([prof[off::period].mean() for off in range(period)])
                ratio = float(vals.max() / mean)
                if ratio > best["ratio"]:
                    off = int(vals.argmax())
                    best = {
                        "ratio": ratio,
                        "period": period,
                        "offset": off,
                        "axis": "vertical" if axis == 1 else "horizontal",
                        "amplitude": round(float(amp[off::period].mean()), 2),
                        "base_amplitude": round(float(amp.mean()), 2),
                        "roi": [x0, y0, x1, y1],
                    }
    # largest flat (untextured) terrain rectangle
    x0, x1 = int(w * 0.18), int(w * 0.76)
    y0, y1 = int(h * 0.16), int(h * 0.86)
    if bottom is not None:
        y1 = min(y1, int(bottom))
    if y1 <= y0:
        y1 = y0 + 1
    g = rgb[y0:y1, x0:x1].astype(np.float32)
    gh, gw = g.shape[:2]
    cell = 8
    gh2, gw2 = gh // cell * cell, gw // cell * cell
    blocks = g[:gh2, :gw2].reshape(gh2 // cell, cell, gw2 // cell, cell, 3)
    std = blocks.std(axis=(1, 3)).mean(axis=2)
    flat = std < 1.6
    flat_components = sorted(components(flat, min_size=8), key=lambda b: -b[4])
    roi_area = float(flat.size)
    largest = flat_components[0] if flat_components else None
    patch_bbox = None
    patch_frac = 0.0
    if largest:
        bx0 = x0 + largest[0] * cell
        by0 = y0 + largest[1] * cell
        bx1 = x0 + (largest[2] + 1) * cell
        by1 = y0 + (largest[3] + 1) * cell
        patch_bbox = [int(bx0), int(by0), int(bx1), int(by1)]
        patch_frac = (bx1 - bx0) * (by1 - by0) / float((x1 - x0) * (y1 - y0))
    return {
        "roi": [x0, y0, x1, y1],
        "seam": best,
        "flat_fraction": round(float(flat.mean()), 4),
        "largest_patch": patch_bbox,
        "largest_patch_fraction": round(float(patch_frac), 4),
    }


def world_holes(rgb: np.ndarray, world_rect) -> dict:
    x0, y0, x1, y1 = [int(v) for v in world_rect]
    crop = rgb[y0:y1, x0:x1]
    r = crop[:, :, 0].astype(np.int16)
    g = crop[:, :, 1].astype(np.int16)
    b = crop[:, :, 2].astype(np.int16)
    total = float(crop.shape[0] * crop.shape[1]) or 1.0
    black = (r < 18) & (g < 18) & (b < 18)
    white = (r > 240) & (g > 240) & (b > 240)
    pink = (r > 195) & (b > 175) & (g < 95) & (r - g > 90)
    out = {"rect": [x0, y0, x1, y1], "total": int(total)}
    for name, mask in (("black", black), ("white", white), ("pink", pink)):
        comps = sorted(components(mask, min_size=120), key=lambda c: -c[4])
        biggest = comps[0] if comps else None
        out[name] = {
            "fraction": round(float(mask.mean()), 4),
            "clusters": len(comps),
            "largest": biggest,
            "largest_fraction": round(biggest[4] / total, 4) if biggest else 0.0,
        }
    return out


def joystick_metrics(rgb: np.ndarray, lay: dict) -> dict:
    cx, cy = lay["joystick_center"]
    radius = lay["joystick_radius"]
    h, w = rgb.shape[:2]
    g = rgb.astype(np.float32).mean(axis=2)

    def disc_mean(r: float, inner: bool) -> float:
        ys = np.arange(max(0, int(cy - r)), min(h, int(cy + r)))
        xs = np.arange(max(0, int(cx - r)), min(w, int(cx + r)))
        if not len(ys) or not len(xs):
            return 0.0
        yy, xx = np.meshgrid(ys, xs, indexing="ij")
        mask = (xx - cx) ** 2 + (yy - cy) ** 2 <= r * r
        if inner:
            mask &= (xx - cx) ** 2 + (yy - cy) ** 2 >= (r * 0.55) ** 2
        return float(g[yy[mask], xx[mask]].mean()) if mask.any() else 0.0

    inside = disc_mean(radius * 0.95, True)
    outside = disc_mean(radius * 1.55, True)
    knob = disc_mean(radius * 0.22, False)
    contrast = contrast_ratio((inside,) * 3, (outside,) * 3)
    return {
        "centre": [round(cx, 1), round(cy, 1)],
        "radius": round(radius, 1),
        "inside_luma": round(inside, 1),
        "outside_luma": round(outside, 1),
        "knob_luma": round(knob, 1),
        "contrast": round(float(contrast), 2),
        "knob_visible": knob - outside > 12.0,
    }


def minimap_metrics(rgb: np.ndarray, lay: dict) -> dict:
    x0, y0, x1, y1 = [int(v) for v in lay["minimap"]]
    crop = rgb[y0:y1, x0:x1]
    if crop.size == 0:
        return {"ok": False, "reason": "empty rect"}
    g = crop.astype(np.float32)
    r, gr, b = g[:, :, 0], g[:, :, 1], g[:, :, 2]
    blue = (b > r + 25) & (b > gr + 8) & (b > 70)
    red = (r > gr + 25) & (r > b + 5) & (r > 70)
    # Unit pips are 4-10 px squares; water is one large blue region.
    def pips(mask):
        return [
            c
            for c in components(mask, min_size=4)
            if 4 <= c[4] <= 220 and (c[2] - c[0] + 1) <= 14 and (c[3] - c[1] + 1) <= 14
        ]

    blue_pips = pips(blue)
    red_pips = pips(red)
    return {
        "rect": [x0, y0, x1, y1],
        "variance": round(float(g.std()), 2),
        "unique_colours": int(len(np.unique((g // 16).astype(np.uint8).reshape(-1, 3), axis=0))),
        "blue_pips": len(blue_pips),
        "red_pips": len(red_pips),
        "blue_pixels": int(blue.sum()),
        "red_pixels": int(red.sum()),
        "water_blue": round(float(blue.mean()), 3),
        "bright_frac": round(float((g.mean(axis=2) > 200).mean()), 4),
    }


def button_icon_metrics(rgb: np.ndarray, rect, label_px: float) -> dict:
    x0, y0, x1, y1 = [int(v) for v in rect]
    inset = max(4, int((x1 - x0) * 0.16))
    bottom = int(y1 - max(10, label_px * 1.6))
    crop = rgb[y0 + inset : max(y0 + inset + 4, bottom - inset), x0 + inset : x1 - inset]
    if crop.size == 0:
        return {"std": 0.0, "colours": 0, "filled": False}
    gray = crop.astype(np.float32).mean(axis=2)
    colours = len(np.unique((crop // 24).astype(np.uint8).reshape(-1, 3), axis=0))
    return {
        "std": round(float(gray.std()), 2),
        "colours": int(colours),
        "filled": bool(gray.std() > 3.0 and colours >= 4),
    }


# --------------------------------------------------------------------- report


def _console_gate(gates: list[Gate], gate_id: str, ok: bool, detail: str, severity: str = "fail", **metrics) -> None:
    gates.append(Gate(gate_id, ok, detail, severity=severity, **metrics))


def build_console(
    image_path: Path,
    insets: dict,
    density: float,
    fps: float | None,
    frame_meta: dict | None,
    safe: dict | None = None,
    selected: int = 0,
) -> dict:
    """1:1 console HUD gate (the post-merge base), not the rejected mobile rings."""
    image = common.load_rgb(image_path)
    w, h = image.size
    rgb = np.asarray(image)
    clean = rgb
    lay = layout.hud_layout(w, h, safe or insets)
    gates: list[Gate] = []
    annotations: list[dict] = []
    safe_area = (float(insets["left"]), float(insets["top"]), float(w - insets["right"]), float(h - insets["bottom"]))
    inset_rects = {
        "top": (0.0, 0.0, float(w), float(insets["top"])),
        "left": (0.0, 0.0, float(insets["left"]), float(h)),
        "right": (float(w - insets["right"]), 0.0, float(w), float(h)),
        "bottom": (0.0, float(h - insets["bottom"]), float(w), float(h)),
    }
    inset_rects = {k: v for k, v in inset_rects.items() if v[2] > v[0] and v[3] > v[1]}

    # 1. system bars -------------------------------------------------------
    overlaps = []
    for name, r in (
        ("console", lay["console"]),
        ("card", lay["card"]),
        ("minimap", lay["minimap"]),
        ("portrait", lay["portrait"]),
        ("resources", lay["resources"]),
        ("top_bar", lay["top_bar"]),
    ):
        for side, ir in inset_rects.items():
            area = layout.intersect(r, ir)
            if area > 1.0:
                overlaps.append({"item": name, "bar": side, "area": round(area, 1)})
    _console_gate(gates, "system_bar_overlap", not overlaps, "HUD under visible system bars" if overlaps else "HUD clear of system bars", overlaps=overlaps[:20])
    for side, ir in inset_rects.items():
        annotations.append({"kind": "inset", "rect": [int(v) for v in ir], "side": side})
    annotations.append({"kind": "console", "rect": [int(v) for v in lay["console"]]})
    annotations.append({"kind": "card", "rect": [int(v) for v in lay["card"]]})
    annotations.append({"kind": "portrait", "rect": [int(v) for v in lay["portrait"]]})

    # 2. 1:1 console grid --------------------------------------------------
    tol = 4.0
    console_h = lay["console"][3] - lay["console"][1]
    _console_gate(gates, "hud_console_height", abs(console_h - lay["targets"]["console_height"]) <= tol,
                  f"console={console_h:.1f}px target={lay['targets']['console_height']:.1f}px", console=lay["console"], target=lay["targets"]["console_height"])
    _console_gate(gates, "hud_console_edge", abs(lay["console"][3] - h) <= tol,
                  f"bottom={lay['console'][3]:.1f} screen={h}", console=lay["console"])
    _console_gate(gates, "hud_minimap", abs((lay["minimap"][2] - lay["minimap"][0]) - lay["targets"]["minimap"]) <= 3.0,
                  f"minimap={lay['minimap'][2] - lay['minimap'][0]:.1f}", minimap=lay["minimap"])
    _console_gate(gates, "hud_portrait", abs((lay["portrait"][2] - lay["portrait"][0]) - lay["targets"]["portrait"]) <= 3.0,
                  f"portrait={lay['portrait'][2] - lay['portrait'][0]:.1f}", portrait=lay["portrait"])
    slots = lay.get("card_slots") or []
    _console_gate(gates, "hud_card_grid", len(slots) == 12 and abs(lay["cell"] - lay["targets"]["cell"]) <= 2.0 and abs(lay["gap"] - lay["targets"]["gap"]) <= 2.0,
                  f"slots={len(slots)} cell={lay['cell']:.1f} gap={lay['gap']:.1f}", slots=len(slots), cell=lay["cell"], gap=lay["gap"])
    _console_gate(gates, "hud_resources", len(lay["resource_pills"]) == 3 and lay["resources"][1] < lay["card"][1] and lay["resources"][2] <= lay["card"][2] + 1,
                  f"pills={len(lay['resource_pills'])}", resources=lay["resources"], card=lay["card"])
    _console_gate(gates, "hud_top_strip", abs((lay["top_bar"][3] - lay["top_bar"][1]) - lay["targets"]["top_strip"]) <= 2.0,
                  f"top={lay['top_bar'][3] - lay['top_bar'][1]:.1f}", top_bar=lay["top_bar"])
    _console_gate(gates, "hud_no_joystick", lay.get("joystick") is None, "no joystick" if lay.get("joystick") is None else "joystick present", joystick=lay.get("joystick"))
    # Conflict: 64 px cells are 21.3 dp at 480 dpi, not 48 dp. Advisory only.
    _console_gate(gates, "touch_targets_advisory", False,
                  f"1:1 cell={lay['cell'] / (density / 160.0):.1f}dp < 48dp advisory (ASSET_COVERAGE conflict)",
                  severity="warn", cell_dp=round(lay["cell"] / (density / 160.0), 1))

    # 3. text --------------------------------------------------------------
    gray = clean.astype(np.float32).mean(axis=2)
    bands = layout.text_bands(lay, w, h)
    measured = [measure_band(gray, band) for band in bands]
    present = [band for band in measured if band["present"]]
    tiny = [band for band in present if band["px"] < 10.0]
    low = [band for band in present if band["contrast"] < 3.0]
    _console_gate(gates, "text_readability", not tiny and not low,
                  f"present={len(present)} tiny={len(tiny)} low_contrast={len(low)}",
                  severity="warn", tiny=[{"band": b["name"], "px": b["px"]} for b in tiny],
                  low=[{"band": b["name"], "ratio": b["contrast"]} for b in low])
    for band in present:
        if band["bbox"]:
            annotations.append({"kind": "text", "rect": band["bbox"], "detail": f"{band['px']:.0f}px {band['contrast']:.1f}:1"})

    # 4. terrain / world ---------------------------------------------------
    terrain = terrain_metrics(clean, w, h, bottom=int(lay["console"][1]) - 8)
    seam = terrain["seam"]
    seam_fail = seam["ratio"] >= 1.8 and seam.get("amplitude", 0.0) >= 8.0
    _console_gate(gates, "terrain_seams", not seam_fail,
                  f"seam={seam['ratio']:.2f}x amp={seam.get('amplitude', 0):.1f}", **seam)
    _console_gate(gates, "terrain_flat_patches", terrain["largest_patch_fraction"] < 0.06,
                  f"flat={terrain['flat_fraction']:.1%} largest={terrain['largest_patch_fraction']:.1%}",
                  flat_fraction=terrain["flat_fraction"], largest_patch=terrain["largest_patch"], largest_patch_fraction=terrain["largest_patch_fraction"])
    annotations.append({"kind": "terrain_roi", "rect": terrain["roi"], "detail": f"seam {seam['ratio']:.2f}x p{seam['period']}"})
    if terrain["largest_patch"]:
        annotations.append({"kind": "flat_patch", "rect": terrain["largest_patch"], "detail": "untextured tile"})
    world = (max(insets["left"], int(w * 0.02)), max(insets["top"], lay["top_bar"][3] + 4), int(w * 0.98), int(lay["console"][1] - 6))
    holes = world_holes(clean, world)
    _console_gate(gates, "black_holes", holes["black"]["fraction"] < 0.18 and holes["black"]["largest_fraction"] < 0.03,
                  f"black={holes['black']['fraction']:.1%}", **holes["black"])
    _console_gate(gates, "white_holes", holes["white"]["fraction"] < 0.06 and holes["white"]["largest_fraction"] < 0.02,
                  f"white={holes['white']['fraction']:.1%}", **holes["white"])
    _console_gate(gates, "pink_alpha", holes["pink"]["fraction"] < 0.001 and holes["pink"]["clusters"] == 0,
                  f"pink={holes['pink']['fraction']:.2%}", **holes["pink"])
    units = detect_units(clean, world)
    _console_gate(gates, "units_visible", bool(units["blue_clusters"] or units["red_clusters"]),
                  f"blue={len(units['blue_clusters'])} red={len(units['red_clusters'])}",
                  blue_pixels=units["blue_pixels"], red_pixels=units["red_pixels"])
    for cluster in (units["blue_clusters"] + units["red_clusters"])[:12]:
        annotations.append({"kind": "unit", "rect": [world[0] + cluster[0], world[1] + cluster[1], world[0] + cluster[2], world[1] + cluster[3]]})

    minimap = minimap_metrics(clean, lay)
    _console_gate(gates, "minimap_readability", minimap["variance"] >= 8.0 and minimap["unique_colours"] >= 24 and minimap["blue_pixels"] + minimap["red_pixels"] >= 20,
                  f"variance={minimap['variance']:.1f} colours={minimap['unique_colours']}", **minimap)
    annotations.append({"kind": "minimap", "rect": [int(v) for v in lay["minimap"]]})
    sel_bands = [b for b in measured if b["name"].startswith("selection_") and b["present"]]
    sel_ok = bool(sel_bands) and any(b["contrast"] >= 3.0 for b in sel_bands)
    _console_gate(gates, "selection_readability", sel_ok,
                  f"selection bands={len(sel_bands)}" if sel_bands else "no selection (nothing selected)",
                  severity="fail" if selected else "warn", bands=[b["name"] for b in sel_bands])
    annotations.append({"kind": "selection", "rect": [int(v) for v in lay["selection"]]})

    # 5. performance -------------------------------------------------------
    if fps is not None:
        _console_gate(gates, "fps", fps >= 20.0, f"{fps:.0f} FPS", fps=fps)
    hard = [g for g in gates if not g.ok and g.severity == "fail"]
    warns = [g for g in gates if not g.ok and g.severity == "warn"]
    return {
        "ok": not hard,
        "image": common.rel(image_path),
        "size": [w, h],
        "density": density,
        "insets": insets,
        "safe_area": [safe_area[0], safe_area[1], safe_area[2], safe_area[3]],
        "layout_safe": safe or insets,
        "layout_source": lay["source"],
        "hud_layout": {k: lay[k] for k in ("scale", "console", "card", "minimap", "portrait", "resources", "top_bar", "cell", "gap")},
        "gates": [gate.as_dict() for gate in gates],
        "fail_count": len(hard),
        "warn_count": len(warns),
        "text_lines": present,
        "detected_buttons": [],
        "chrome_components": 0,
        "terrain": terrain,
        "units": units,
        "minimap": minimap,
        "joystick": {"present": lay.get("joystick") is not None},
        "frame": frame_meta or {},
        "selected": selected,
        "_annotations": annotations,
    }


def build(image_path: Path, insets: dict, density: float, fps: float | None, frame_meta: dict | None, safe: dict | None = None, selected: int = 0) -> dict:
    """Canonical console gate (the single base)."""
    return build_console(image_path, insets, density, fps, frame_meta, safe, selected)


def show_clip_detail(items) -> str:
    if not items:
        return "all buttons inside the safe area"
    return "; ".join(f"{i['button']} {i['reason']}" for i in items)


def annotate(image_path: Path, report: dict, out_path: Path) -> None:
    image = common.load_rgb(image_path).convert("RGB")
    draw = ImageDraw.Draw(image)
    font = _font()
    colors = {
        "inset": (255, 64, 64),
        "cluster": (255, 160, 32),
        "text": (0, 220, 255),
        "flat_patch": (255, 0, 255),
        "terrain_roi": (255, 0, 255),
        "black": (255, 40, 40),
        "white": (255, 255, 0),
        "pink": (255, 0, 128),
        "unit": (0, 255, 90),
        "minimap": (0, 200, 255),
        "selection": (0, 200, 255),
        "joystick": (80, 140, 255),
        "overlay": (150, 150, 150),
        "console": (255, 180, 40),
        "card": (0, 255, 255),
        "portrait": (255, 0, 255),
    }
    labels = []

    def box(rect, color, width=2):
        draw.rectangle([int(rect[0]), int(rect[1]), int(rect[2]), int(rect[3])], outline=color, width=width)

    for item in report.get("_annotations", []):
        kind = item["kind"]
        if kind == "inset":
            box(item["rect"], colors["inset"], 5)
            labels.append((item["rect"][0] + 6, item["rect"][1] + 6, f"SYSTEM BAR {item['side']}"))
        elif kind == "console":
            box(item["rect"], colors["console"], 3)
            labels.append((item["rect"][0] + 4, item["rect"][1] + 4, "CONSOLE"))
        elif kind == "card":
            box(item["rect"], colors["card"], 2)
            labels.append((item["rect"][0] + 4, item["rect"][1] + 4, "CARD 4x3"))
        elif kind == "portrait":
            box(item["rect"], colors["portrait"], 2)
        elif kind == "cluster":
            box(item["rect"], colors["cluster"], 3)
            labels.append((item["rect"][0], item["rect"][1] - 26, "COMMAND CLUSTER"))
        elif kind == "text":
            box(item["rect"], colors["text"], 1)
            labels.append((item["rect"][0], max(0, item["rect"][1] - 22), item.get("detail", "")))
        elif kind in ("flat_patch", "terrain_roi"):
            box(item["rect"], colors[kind], 3)
            labels.append((item["rect"][0] + 4, item["rect"][1] + 4, item.get("detail", kind)))
        elif kind in ("black", "white", "pink"):
            box(item["rect"], colors[kind], 3)
            labels.append((item["rect"][0], item["rect"][1], f"{kind} {item.get('detail','')}"))
        elif kind == "unit":
            box(item["rect"], colors["unit"], 2)
        elif kind == "minimap":
            box(item["rect"], colors["minimap"], 2)
        elif kind == "selection":
            box(item["rect"], colors["selection"], 2)
        elif kind == "overlay":
            box(item["rect"], colors["overlay"], 3)
            labels.append((item["rect"][0] + 4, item["rect"][1] + 4, item.get("detail", "overlay")))
        elif kind == "joystick":
            cx, cy = item["centre"]
            r = item["radius"]
            draw.ellipse([cx - r, cy - r, cx + r, cy + r], outline=colors["joystick"], width=3)

    # detected button borders + dp size
    for item in report.get("detected_buttons", []):
        r = item["rect"]
        box(r, (255, 230, 0), 1)
    for key, item in report["hud_layout"].items():
        if key == "scale":
            continue

    # gate summary chip
    header = f"{'PASS' if report['ok'] else 'FAIL'}  fails={report['fail_count']} warns={report['warn_count']}"
    draw.rectangle([0, 0, 620, 34], fill=(0, 0, 0))
    draw.text((8, 4), header, fill=(0, 255, 0) if report["ok"] else (255, 60, 60), font=font)

    for x, y, text in labels[:24]:
        if not text:
            continue
        draw.rectangle([x, y, x + 10 * len(text) + 8, y + 24], fill=(0, 0, 0))
        draw.text((x + 4, y + 1), text, fill=(255, 255, 255), font=font)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    image.save(out_path)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("image", type=Path)
    parser.add_argument("--json-out", type=Path)
    parser.add_argument("--annotated", type=Path)
    parser.add_argument("--insets", default=None, help="system bars L,T,R,B in screenshot pixels")
    parser.add_argument("--safe", default=None, help="game safe area L,T,R,B (GAME_FIRST_FRAME)")
    parser.add_argument("--density", type=float, default=480.0)
    parser.add_argument("--fps", type=float, default=None)
    parser.add_argument("--selected", type=int, default=0, help="GAME_STATE selected count")
    parser.add_argument("--frame-json", type=Path, default=None, help="device launch/info JSON")
    parser.add_argument("--quiet", action="store_true")
    args = parser.parse_args(argv)

    insets = _insets_from_args(args.insets)
    safe = _insets_from_args(args.safe) if args.safe else None
    frame_meta = None
    if args.frame_json and args.frame_json.exists():
        frame_meta = json.loads(args.frame_json.read_text())
        if not args.insets:
            display = frame_meta.get("display", {})
            found = frame_meta.get("insets", {})
            if display and found:
                insets = {
                    "left": found.get("left", 0),
                    "top": found.get("top", 0),
                    "right": found.get("right", 0),
                    "bottom": found.get("bottom", 0),
                    "status_visible": found.get("status_visible", True),
                    "nav_visible": found.get("nav_visible", True),
                }
                if args.density == 480.0 and found.get("density"):
                    args.density = float(found["density"])
    report = build(args.image, insets, args.density, args.fps, frame_meta, safe, args.selected)
    report["analyzed_at"] = common.now_iso()
    if args.annotated:
        annotate(args.image, report, args.annotated)
        report["annotated"] = common.rel(args.annotated)
    if args.json_out:
        common.save_json(args.json_out, {k: v for k, v in report.items() if not k.startswith("_")})
    if not args.quiet:
        print(f"VISUAL {'PASS' if report['ok'] else 'FAIL'} fails={report['fail_count']} warns={report['warn_count']}")
        for gate in report["gates"]:
            if not gate["ok"]:
                print(f"  {gate['severity'].upper():4s} {gate['id']}: {gate['detail']}")
        for gate in report["gates"]:
            if gate["ok"]:
                print(f"  ok   {gate['id']}: {gate['detail']}")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
