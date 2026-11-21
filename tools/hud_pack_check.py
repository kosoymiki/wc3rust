#!/usr/bin/env python3
"""Deterministic gate for the final-pack HUD frame (ledger 457, re-pointed 2026-11-18).

The layout engine was rewritten to dp units (`crates/wc3-game/src/mobile_hud.rs`)
and `examples/hud_layout.rs` now emits
`analysis/hudskin/hud-layout-matrix.json`: per viewport, the density, the
dp/px viewport, the console/top-bar band, the per-state raw geometry (every
interactive rect, the card slots, the group slots) and the safe insets. The old
`pack-layout-<w>x<h>.json` element table (draws, UVs, 9-slice corners, four
console sections) no longer exists: the renderer builds its draw plan at draw
time and the engine is pure dp geometry. This checker was therefore re-pointed
at the matrix + the rendered screenshots, and the rules that only made sense for
the retired table were deleted instead of being re-implemented against nothing.

Rules kept, re-derived here from the matrix's raw rects (never from its verdicts):

1. completeness: 92 assets, 5 fonts, 20 atlas cells, file presence and PNG
   dimensions equal to the manifest;
2. the 11 rows whose insets cannot be sliced are classified unsliced (known
   pack defects) and, when drawn, their `HUD_PACK insets-ignored <file>` log
   line must exist;
3. touch floor: every interactive rect (all states, including card and group
   slots) is at least `touch_floor_dp` (48 dp) on its shorter side, and the
   row's `min_target_dp` aggregate equals the minimum recomputed from the rects;
4. command card: every state that reaches the card has exactly 12 slots laid
   out as 4 columns x 3 rows;
5. safe-area containment: every interactive rect stays inside the safe frame
   (framebuffer minus the platform insets);
6. row arithmetic: `dp_viewport * density == framebuffer_px`,
   `console_px == console_dp * density`, `hud_pct + viewport_pct == 100`;
7. pixels: the screenshot matches the framebuffer, contains no placeholder
   magenta wash (whole frame) and the console band is real art (unique colours),
   with no exact-magenta placeholder pixels;
8. optional `--baseline` regression: SSIM-lite between two frames >= 0.90.

Deleted with the retired table (do not re-add without the data):
* the four console sections summing to the viewport width/height — the dp
  engine has no four-section split any more;
* 9-slice corner source/destination equal to the manifest inset — the checker
  no longer receives draw quads or UVs; that contract belongs to the renderer;
* single-image stretch <= 1.15x — same reason (no draw quads);
* `MIN_TEXT_PX` text-run floor — the matrix carries no text rects; text
  legibility is covered by `tools/hud_text_check.py` (its own lane);
* legacy procedural stone colour — the final pack is the only art now.

Usage:
    # reference viewport, defaults for every path
    python3 tools/hud_pack_check.py

    # every matrix row (needs a `none`-state screenshot per row)
    python3 tools/hud_pack_check.py --viewport all

    # explicit paths (CI-style)
    python3 tools/hud_pack_check.py \
      --manifest analysis/hudskin/final/wc3rust-hud-final/manifest.json \
      --matrix analysis/hudskin/hud-layout-matrix.json \
      --viewport device-2414x1080 \
      --png analysis/hudskin/screens/device-2414x1080-none-2414x1080.png

Exit status: 0 pass, 1 any violated rule, 2 missing input / bad arguments.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

DEFAULT_MANIFEST = "analysis/hudskin/final/wc3rust-hud-final/manifest.json"
DEFAULT_MATRIX = "analysis/hudskin/hud-layout-matrix.json"
DEFAULT_VIEWPORT = "device-2414x1080"
DEFAULT_SCREENS = "analysis/hudskin/screens"

EXPECTED_ASSETS = 92
EXPECTED_FONTS = 5
EXPECTED_ATLAS_ENTRIES = 20
CARD_COLUMNS = 4
CARD_ROWS = 3
CARD_SLOTS = CARD_COLUMNS * CARD_ROWS
TOUCH_TOL_PX = 0.75
RECT_TOL_PX = 0.75
MAGENTA_FRAME_LIMIT = 0.0005  # 0.05 % of the whole frame
MAGENTA_CONSOLE_LIMIT = 0.002  # 0.2 % of the console band
MIN_CONSOLE_COLOURS = 2000

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


# --------------------------------------------------------------------- pack


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
    return not sliceable(asset)


def sliced(asset):
    return sliceable(asset) and not single_image(asset) and any(value > 0 for value in insets(asset))


def pack_summary(assets):
    items = list(assets)
    return {
        "assets": len(items),
        "sliced": sum(1 for asset in items if sliced(asset)),
        "unsliced": sum(1 for asset in items if not sliced(asset)),
        "insets_ignored": sum(1 for asset in items if not sliceable(asset)),
    }


def check_manifest(manifest_path: Path, log_text: str | None, failures: list[str]) -> dict:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    root = manifest_path.parent

    if len(manifest["assets"]) != EXPECTED_ASSETS:
        failures.append(f"asset count {len(manifest['assets'])} != {EXPECTED_ASSETS}")
    if len(manifest.get("font_files") or []) != EXPECTED_FONTS:
        failures.append(f"font_files {len(manifest.get('font_files') or [])} != {EXPECTED_FONTS}")
    entries = (manifest.get("atlas") or {}).get("commandbuttons", {}).get("entries") or []
    if len(entries) != EXPECTED_ATLAS_ENTRIES:
        failures.append(f"commandbuttons atlas entries {len(entries)} != {EXPECTED_ATLAS_ENTRIES}")

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

    guard_failures = {asset["name"] for asset in manifest["assets"] if not sliceable(asset)}
    if guard_failures != KNOWN_DEFECTS:
        failures.append(
            "known-defect table drift: "
            f"manifest-only={sorted(guard_failures - KNOWN_DEFECTS)} "
            f"table-only={sorted(KNOWN_DEFECTS - guard_failures)}"
        )
    if log_text is not None:
        by_name = {asset["name"]: asset for asset in manifest["assets"]}
        for name in sorted(KNOWN_DEFECTS):
            asset = by_name.get(name)
            if asset is None:
                continue
            needle = f"HUD_PACK insets-ignored {asset['file']}"
            if needle not in log_text:
                failures.append(f"defect {name}: missing log line '{needle}'")
    return manifest


# ------------------------------------------------------------------- matrix


def rect(quad) -> tuple[float, float, float, float]:
    if not isinstance(quad, list) or len(quad) != 4:
        raise ValueError(f"bad rect {quad!r}")
    return float(quad[0]), float(quad[1]), float(quad[2]), float(quad[3])


def inside(rect4, outer, tol=RECT_TOL_PX) -> bool:
    x, y, w, h = rect4
    ox, oy, ow, oh = outer
    return (
        x >= ox - tol
        and y >= oy - tol
        and x + w <= ox + ow + tol
        and y + h <= oy + oh + tol
    )


def row_arithmetic(row, failures: list[str]) -> None:
    label = row.get("label", "?")
    try:
        w, h = (float(v) for v in row["framebuffer_px"])
        density = float(row["density_scale"])
        dp_w, dp_h = (float(v) for v in row["dp_viewport"])
        console_dp = float(row["console_dp"])
        console_px = float(row["console_px"])
        hud_pct = float(row["hud_pct"])
        viewport_pct = float(row["viewport_pct"])
        safe = [float(v) for v in row["safe_px"]]
    except (KeyError, TypeError, ValueError) as error:
        failures.append(f"{label}: matrix row incomplete ({error})")
        return
    if density <= 0:
        failures.append(f"{label}: density {density} <= 0")
        return
    if abs(dp_w * density - w) > 1.5 or abs(dp_h * density - h) > 1.5:
        failures.append(
            f"{label}: dp {dp_w:.2f}x{dp_h:.2f} * {density} != framebuffer {w:.0f}x{h:.0f}"
        )
    if abs(console_dp * density - console_px) > TOUCH_TOL_PX:
        failures.append(
            f"{label}: console {console_dp:.2f}dp * {density} != {console_px:.2f}px"
        )
    if abs(hud_pct + viewport_pct - 100.0) > 0.1:
        failures.append(f"{label}: hud_pct {hud_pct:.2f} + viewport_pct {viewport_pct:.2f} != 100")
    if len(safe) != 4:
        failures.append(f"{label}: safe_px has {len(safe)} values")
    if row.get("gate") != "ok":
        failures.append(f"{label}: internal gate is {row.get('gate')!r}, expected 'ok'")


def check_row_geometry(row, floor_dp: float, failures: list[str]) -> tuple[int, int]:
    """Re-derive touch floor, 4x3 card grid and safe containment from raw rects.

    Returns (states_checked, card_slots_seen). Everything is checked in px; the
    density from the row converts the 48 dp floor.
    """
    label = row.get("label", "?")
    missing = [key for key in ("density_scale", "framebuffer_px", "safe_px", "states") if key not in row]
    if missing:
        failures.append(
            f"{label}: matrix row predates the dp matrix schema (missing {missing}); "
            "regenerate with `scripts/h2-cargo run -p wc3-game --example hud_layout --offline`"
        )
        return 0, 0
    density = float(row.get("density_scale") or 0.0)
    floor_px = float(floor_dp) * density
    w, h = (float(v) for v in row["framebuffer_px"])
    sl, st, sr, sb = (float(v) for v in row["safe_px"])
    safe_frame = [sl, st, w - sl - sr, h - st - sb]
    states = row.get("states")
    if not isinstance(states, list) or not states:
        failures.append(f"{label}: matrix row has no per-state geometry")
        return 0, 0

    total_slots = 0
    recomputed_min = float("inf")
    for state in states:
        slug = state.get("slug", "?")
        interactive = state.get("interactive")
        if not isinstance(interactive, list) or not interactive:
            failures.append(f"{label}/{slug}: no interactive rects exported")
            continue
        for item in interactive:
            try:
                zone, quad = item["id"], rect(item["rect"])
            except (KeyError, TypeError, ValueError) as error:
                failures.append(f"{label}/{slug}: bad interactive row ({error})")
                continue
            shorter = min(quad[2], quad[3])
            recomputed_min = min(recomputed_min, shorter)
            if shorter + TOUCH_TOL_PX < floor_px:
                failures.append(
                    f"{label}/{slug}/{zone}: target {quad[2]:.1f}x{quad[3]:.1f}px "
                    f"< {floor_px:.1f}px ({floor_px / density:.0f}dp)"
                )
            if not inside(quad, safe_frame):
                failures.append(
                    f"{label}/{slug}/{zone}: rect {quad} leaves the safe frame {safe_frame}"
                )

        slots = [rect(quad) for quad in state.get("card_slots") or []]
        if slots:
            total_slots += len(slots)
            if len(slots) != CARD_SLOTS:
                failures.append(f"{label}/{slug}: {len(slots)} card slots != {CARD_SLOTS}")
            columns = len({round(slot[0], 1) for slot in slots})
            rows = len({round(slot[1], 1) for slot in slots})
            if (columns, rows) != (CARD_COLUMNS, CARD_ROWS):
                failures.append(
                    f"{label}/{slug}: card grid is {columns}x{rows}, expected "
                    f"{CARD_COLUMNS}x{CARD_ROWS}"
                )
            for slot in slots:
                if min(slot[2], slot[3]) + TOUCH_TOL_PX < floor_px:
                    failures.append(
                        f"{label}/{slug}/card_slot: {slot[2]:.1f}x{slot[3]:.1f}px < {floor_px:.1f}px"
                    )

    declared = row.get("min_target_dp")
    if declared is not None and np.isfinite(recomputed_min):
        recomputed_dp = recomputed_min / density if density else float("nan")
        if abs(float(declared) - recomputed_dp) > TOUCH_TOL_PX:
            failures.append(
                f"{label}: min_target_dp {float(declared):.2f} != recomputed {recomputed_dp:.2f}"
            )
    return len(states), total_slots


# ------------------------------------------------------------------- pixels


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


def check_pixels(png_path: Path, row, baseline: Path | None, failures: list[str]) -> None:
    label = row.get("label", "?")
    w, h = (int(round(float(v))) for v in row["framebuffer_px"])
    console_px = int(round(float(row["console_px"])))
    with Image.open(png_path) as handle:
        image = handle.convert("RGB")
    if image.size != (w, h):
        failures.append(f"{png_path.name}: size {image.size} != framebuffer {(w, h)}")
        return
    pixels = np.asarray(image)
    red = pixels[:, :, 0].astype(int)
    green = pixels[:, :, 1].astype(int)
    blue = pixels[:, :, 2].astype(int)

    magenta_mask = (red > 190) & (blue > 190) & (green < 90)
    magenta = float(magenta_mask.mean())
    if magenta > MAGENTA_FRAME_LIMIT:
        failures.append(f"{label}: magenta/placeholder pixels cover {magenta * 100:.3f}% of the frame")
    exact = float(np.mean((red == 255) & (green == 0) & (blue == 255)))
    if exact > 0.0:
        failures.append(f"{label}: exact magenta (255,0,255) covers {exact * 100:.4f}% of the frame")

    band = pixels[max(0, h - console_px):, :, :]
    unique = len(np.unique(band.reshape(-1, 3), axis=0))
    band_magenta = float(
        ((band[:, :, 0] > 190) & (band[:, :, 2] > 190) & (band[:, :, 1] < 90)).mean()
    )
    if unique < MIN_CONSOLE_COLOURS:
        failures.append(f"{label}: console band has only {unique} colours: flat fill, not art")
    if band_magenta > MAGENTA_CONSOLE_LIMIT:
        failures.append(f"{label}: magenta wash on {band_magenta * 100:.2f}% of the console band")
    print(
        f"{label}: {png_path.name} colours={unique} magenta={magenta * 100:.3f}% "
        f"console_magenta={band_magenta * 100:.3f}%"
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


# --------------------------------------------------------------------- main


def default_png(screens: Path, row) -> Path:
    w, h = (int(round(float(v))) for v in row["framebuffer_px"])
    return screens / f"{row['label']}-none-{w}x{h}.png"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--manifest", type=Path, default=Path(DEFAULT_MANIFEST))
    parser.add_argument("--matrix", type=Path, default=Path(DEFAULT_MATRIX))
    parser.add_argument("--viewport", default=DEFAULT_VIEWPORT, help="row label, or 'all'")
    parser.add_argument("--png", type=Path, default=None, help="screenshot for a single row")
    parser.add_argument("--screens", type=Path, default=Path(DEFAULT_SCREENS))
    parser.add_argument("--log", type=Path)
    parser.add_argument("--baseline", type=Path)
    args = parser.parse_args()

    for path, what in ((args.manifest, "manifest"), (args.matrix, "matrix")):
        if not path.is_file():
            print(f"HUD_PACK_CHECK FAIL reason=missing-{what} path={path}")
            return 2

    matrix = json.loads(args.matrix.read_text(encoding="utf-8"))
    rows = matrix.get("rows") or []
    if not rows:
        print(f"HUD_PACK_CHECK FAIL reason=matrix-has-no-rows path={args.matrix}")
        return 2
    if args.viewport != "all":
        rows = [row for row in rows if row.get("label") == args.viewport]
        if not rows:
            print(f"HUD_PACK_CHECK FAIL reason=unknown-viewport {args.viewport}")
            return 2

    log_text = None
    if args.log and args.log.is_file():
        log_text = args.log.read_text(encoding="utf-8", errors="replace")
    failures: list[str] = []
    manifest = check_manifest(args.manifest, log_text, failures)

    states_total = 0
    slots_total = 0
    for row in rows:
        row_arithmetic(row, failures)
        states, slots = check_row_geometry(
            row, float(matrix.get("touch_floor_dp") or 48.0), failures
        )
        states_total += states
        slots_total += slots
        print(
            f"row {row.get('label')}: {int(row['framebuffer_px'][0])}x"
            f"{int(row['framebuffer_px'][1])} states={states} card_slots={slots} "
            f"min_target={float(row.get('min_target_dp') or 0.0):.1f}dp gate={row.get('gate')}"
        )
        png = args.png if (args.png and len(rows) == 1) else default_png(args.screens, row)
        if not png.is_file():
            failures.append(
                f"{row.get('label')}: missing screenshot {png} (run "
                "`scripts/h2-cargo run -p wc3-game --example hud_layout --offline`)"
            )
            continue
        check_pixels(png, row, args.baseline, failures)

    counts = pack_summary(manifest["assets"])
    print(
        "assets={assets} sliced={sliced} unsliced={unsliced} insets_ignored={insets_ignored} "
        "rows={rows} states={states} card_slots={slots}".format(
            rows=len(rows), states=states_total, slots=slots_total, **counts
        )
    )
    if failures:
        for failure in failures:
            print(f"HUD_PACK_CHECK FAIL {failure}")
        print(f"HUD_PACK_CHECK FAIL count={len(failures)}")
        return 1
    print(f"HUD_PACK_CHECK PASS rows={len(rows)} viewport={args.viewport}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
