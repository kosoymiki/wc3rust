#!/usr/bin/env python3
"""Art-push QA gate (Agent G, ledger 591, 2026-11-21).

Scans rendered PNGs for magenta / alpha-checker placeholder pixels and verifies
headless-reference labels so a procedural mock can never be mistaken for a
device screenshot.

Magenta definitions are the project's canonical ones, not invented here:
  * pink mask  = (r > 195) & (b > 175) & (g < 95) & (r - g > 90)
                 (scripts/qa/analyze.py :: world_holes, gate `pink_alpha`)
  * clusters   = scripts.qa.common.components(mask, min_size=120)  (same as analyze.py)
  * exact key  = (255, 0, 255)   (scripts/qa/analyze.py, tools/hud_pack_check.py)
  * hud mask   = (r > 190) & (b > 190) & (g < 90)  (tools/hud_pack_check.py)

PASS requires: dimensions (when given), pink_pixels == 0, exact_magenta == 0 and
pink_clusters == 0. A headless reference additionally requires the literal
banner text "HEADLESS MOCK" to be OCR-readable in the top band.

Source atlases (extracted retail icon sheets) are input textures, not rendered
frames: `--source-atlas` classifies them `N/A-source-atlas` and still fails on
any exact (255,0,255) chroma-key pixel or a broken file. Pink icon artwork in
the atlas is real game art, not a magenta regression.

Usage (from repo root):
  python3 -m scripts.qa.art_push_magenta_scan \
      --json-out analysis/agent-qa/art-push-20261120/magenta_scan.json \
      --expect-size 2414x1080 --check-label \
      analysis/agent-hud/art-push-20261120/headless-*.png
  python3 -m scripts.qa.art_push_magenta_scan --source-atlas \
      --json-out analysis/agent-qa/ALL_LAYERS_20261121/magenta_scan_source_atlas.json \
      analysis/agent-hud/icons/commandbuttons_atlas.png
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import numpy as np
from PIL import Image

from .common import components

PINK = "project_pink_mask (analyze.py world_holes): r>195 & b>175 & g<95 & r-g>90"
HUD = "hud_pack_check magenta mask: r>190 & b>190 & g<90"
EXACT = "(255,0,255) exact key"


def scan(path: Path, expect_size: tuple[int, int] | None, check_label: bool,
         source_atlas: bool = False) -> dict:
    rec: dict = {"path": str(path)}
    try:
        with Image.open(path) as im:
            rgb = np.asarray(im.convert("RGB"), dtype=np.int16)
        h, w = rgb.shape[:2]
        rec["size"] = [w, h]
        rec["size_ok"] = expect_size is None or (w, h) == expect_size

        r, g, b = rgb[:, :, 0], rgb[:, :, 1], rgb[:, :, 2]
        pink = (r > 195) & (b > 175) & (g < 95) & (r - g > 90)
        hud = (r > 190) & (b > 190) & (g < 90)
        exact = (r == 255) & (g == 0) & (b == 255)
        comps = components(pink, min_size=120)
        rec["pink_pixels"] = int(pink.sum())
        rec["pink_fraction"] = round(float(pink.mean()), 8)
        rec["pink_clusters"] = len(comps)
        rec["pink_largest"] = comps[0] if comps else None
        rec["hud_magenta_pixels"] = int(hud.sum())
        rec["exact_magenta_pixels"] = int(exact.sum())

        # OCR only the top banner band — the mock generator writes the label there.
        ocr = ""
        ocr_rc = None
        if check_label:
            banner = Image.fromarray(rgb[: min(80, h), :, :].astype(np.uint8))
            with tempfile.TemporaryDirectory(prefix="artpush-qa-") as td:
                tmp = Path(td) / "banner.png"
                banner.save(tmp)
                try:
                    proc = subprocess.run(
                        ["tesseract", str(tmp), "stdout", "--psm", "6"],
                        capture_output=True, text=True, timeout=30,
                    )
                    ocr = proc.stdout.strip()
                    ocr_rc = proc.returncode
                except (OSError, subprocess.SubprocessError) as exc:
                    ocr = f"OCR_ERROR: {exc}"
                    ocr_rc = -1
        rec["label_ocr"] = ocr[:300]
        rec["label_ocr_rc"] = ocr_rc
        rec["label_ok"] = (None if not check_label else "HEADLESS MOCK" in ocr.upper())

        # analyze.py paints its debug boxes in (255,0,255) (flat_patch,
        # terrain_roi, portrait) onto *.annotated.png. Magenta there is
        # annotation ink, not a render regression: classify, never silent-drop.
        annotated = path.name.endswith(".annotated.png")
        rec["annotation_expected"] = annotated
        if annotated:
            rec["verdict"] = "N/A-annotated"
            rec["note"] = ("magenta is analyze.py debug-box ink "
                           "(scripts/qa/analyze.py:657-669,695-700), not scene pixels")
        elif source_atlas:
            # Input texture extracted from the shipped game, not a rendered
            # frame: pink pixels can be legitimate retail icon artwork. The
            # chroma-key key itself must still be absent.
            rec["source_atlas"] = True
            rec["verdict"] = (
                "N/A-source-atlas"
                if rec["size_ok"] and rec["exact_magenta_pixels"] == 0
                else "FAIL"
            )
            rec["note"] = ("extracted retail icon atlas (input texture, not a "
                           "rendered frame); exact (255,0,255) must be 0")
        else:
            rec["verdict"] = (
                "PASS"
                if rec["size_ok"] and rec["pink_pixels"] == 0
                and rec["pink_clusters"] == 0 and rec["exact_magenta_pixels"] == 0
                and rec["label_ok"] is not False
                else "FAIL"
            )
    except Exception as exc:  # a broken/unreadable image is a hard FAIL, never a skip
        rec["error"] = f"{type(exc).__name__}: {exc}"
        rec["verdict"] = "FAIL"
    return rec


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("images", nargs="+", type=Path)
    ap.add_argument("--expect-size", default=None, help="WxH, e.g. 2414x1080")
    ap.add_argument("--check-label", action="store_true",
                    help="require OCR-readable 'HEADLESS MOCK' banner (headless refs only)")
    ap.add_argument("--source-atlas", action="store_true",
                    help="inputs are extracted retail atlases (input textures, not frames)")
    ap.add_argument("--json-out", type=Path, default=None)
    args = ap.parse_args()

    size = None
    if args.expect_size:
        sw, sh = args.expect_size.lower().split("x")
        size = (int(sw), int(sh))

    results = [scan(p, size, args.check_label, args.source_atlas) for p in args.images]
    n_pass = sum(r["verdict"] == "PASS" for r in results)
    n_annotated = sum(r["verdict"] == "N/A-annotated" for r in results)
    n_source = sum(r["verdict"] == "N/A-source-atlas" for r in results)
    n_fail = sum(r["verdict"] == "FAIL" for r in results)
    report = {
        "tool": "scripts/qa/art_push_magenta_scan.py",
        "generated_local": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "definitions": {"pink": PINK, "hud": HUD, "exact": EXACT,
                        "cluster_min_size": 120,
                        "annotated_classification": (
                            "*.annotated.png -> N/A-annotated; magenta comes from "
                            "analyze.py debug boxes (255,0,255), not scene pixels"),
                        "source_atlas_classification": (
                            "--source-atlas -> N/A-source-atlas; extracted retail icon "
                            "art is an input texture, exact (255,0,255) must still be 0")},
        "expect_size": list(size) if size else None,
        "check_label": args.check_label,
        "source_atlas": args.source_atlas,
        "results": results,
        "summary": {"total": len(results), "pass": n_pass,
                    "annotated_na": n_annotated, "source_atlas_na": n_source,
                    "fail": n_fail},
    }
    out = json.dumps(report, indent=2, ensure_ascii=False)
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(out, encoding="utf-8")
    print(out)
    return 0 if n_fail == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
