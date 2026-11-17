#!/usr/bin/env python3
"""Gallery gate + contact sheets for the full asset corpus.

Consumes the output of `crates/wc3-game/tests/asset_gallery.rs`:

    analysis/agent-qa/gallery/
        models/<Category>/<stem>.ppm   160x160 render of every MDX
        models/manifest.jsonl          per-model flags (missing textures, Stand
                                       corpse/effect geosets, facing)
        models/summary.json            exact category counts
        icons/*.ppm + manifest.jsonl + summary.json
        terrain/*.ppm + manifest.jsonl + summary.json

Outputs contact sheets (models-<category>-NN.png, icons-NN.png, terrain-NN.png)
and `gallery/gates.json` with PASS/FAIL for counts, missing textures, corpse
geosets in Stand, backward facing, black/white/pink/empty thumbnails.

Usage:
    python3 -m scripts.qa.gallery [--gates-only] [--sheet-size 128]
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from . import common

MODEL_CATEGORIES = {
    "Doodads": 1021,
    "Units": 411,
    "Abilities": 230,
    "Buildings": 131,
    "Objects": 99,
    "UI": 97,
    "Environment": 45,
    "SharedModels": 14,
}
TOTAL_MODELS = 2048
TOTAL_ICONS = 691
TOTAL_TERRAIN = 111


def load_manifest(path: Path) -> list[dict]:
    rows = []
    if not path.exists():
        return rows
    for line in path.read_text(errors="replace").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            rows.append({"ok": False, "error": "manifest line is not JSON"})
    return rows


def load_summary(path: Path) -> dict:
    if not path.exists():
        return {}
    try:
        return json.loads(path.read_text())
    except json.JSONDecodeError:
        return {}


def thumb_stats(path: Path) -> dict:
    from PIL import Image

    image = Image.open(path).convert("RGB")
    pixels = list(image.getdata())
    n = max(1, len(pixels))
    black = sum(1 for p in pixels if p[0] < 16 and p[1] < 16 and p[2] < 16)
    white = sum(1 for p in pixels if p[0] > 240 and p[1] > 240 and p[2] > 240)
    pink = sum(1 for p in pixels if p[0] > 180 and p[2] > 180 and p[1] < 90)
    luma = sum(0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2] for p in pixels) / n
    return {
        "black": black / n,
        "white": white / n,
        "pink": pink / n,
        "luma": luma,
        "size": image.size,
    }


def contact_sheets(paths: list[Path], out_dir: Path, prefix: str, size: int, cols: int = 8, rows: int = 6) -> list[str]:
    from PIL import Image, ImageDraw

    out_dir.mkdir(parents=True, exist_ok=True)
    sheets = []
    per_sheet = cols * rows
    label_h = 18
    tile_w = size
    tile_h = size + label_h
    for sheet_index in range(0, len(paths), per_sheet):
        chunk = paths[sheet_index : sheet_index + per_sheet]
        canvas = Image.new("RGB", (cols * tile_w, rows * tile_h), (18, 20, 26))
        draw = ImageDraw.Draw(canvas)
        for index, path in enumerate(chunk):
            col = index % cols
            row = index // cols
            x = col * tile_w
            y = row * tile_h
            try:
                tile = Image.open(path).convert("RGB").resize((size, size), Image.LANCZOS)
                canvas.paste(tile, (x, y))
            except Exception:
                draw.rectangle([x, y, x + size, y + size], fill=(90, 20, 30))
            label = path.stem[:26]
            draw.text((x + 2, y + size + 2), label, fill=(220, 226, 235))
        name = f"{prefix}-{sheet_index // per_sheet + 1:02d}.png"
        canvas.save(out_dir / name)
        sheets.append(common.rel(out_dir / name))
    return sheets


def analyze(gallery: Path, gates_only: bool, sheet_size: int) -> dict:
    gates: list[dict] = []
    rows = load_manifest(gallery / "models" / "manifest.jsonl")
    model_rows = [r for r in rows if r.get("kind") == "model"]
    models_built = len(model_rows) > 0
    ok_models = [r for r in model_rows if r.get("ok")]
    summary = load_summary(gallery / "models" / "summary.json")
    category_counts = {
        name: sum(1 for r in ok_models if r.get("category") == name) for name in MODEL_CATEGORIES
    }
    counts_ok = len([r for r in model_rows if r.get("ok")]) == TOTAL_MODELS and all(
        category_counts[name] == expected for name, expected in MODEL_CATEGORIES.items()
    )
    gates.append(
        {
            "id": "gallery_model_counts",
            "ok": counts_ok,
            "detail": f"models={len(ok_models)}/{TOTAL_MODELS} categories={category_counts}",
            "expected": MODEL_CATEGORIES,
            "observed": category_counts,
        }
    )
    parse_fail = [r for r in model_rows if not r.get("ok")]
    gates.append(
        {
            "id": "gallery_model_parse",
            "ok": not parse_fail,
            "detail": f"{len(parse_fail)} MDX failed to parse/render",
            "failing": [r.get("path") for r in parse_fail][:40],
        }
    )
    missing = [r for r in ok_models if r.get("missing")]
    gates.append(
        {
            "id": "gallery_missing_textures",
            "ok": not missing,
            "detail": f"{len(missing)} models reference missing textures",
            "models": [{"path": r.get("path"), "missing": r.get("missing")} for r in missing][:40],
        }
    )
    corpse = [r for r in ok_models if r.get("corpse_effect_stand")]
    gates.append(
        {
            "id": "gallery_corpse_stand",
            "ok": not corpse,
            "detail": f"{len(corpse)} models keep corpse/effect geosets visible in Stand",
            "models": [{"path": r.get("path"), "geosets": r.get("corpse_effect_stand")} for r in corpse][:40],
        }
    )
    backward = [r for r in ok_models if r.get("facing") == "backward"]
    unknown = [r for r in ok_models if r.get("facing") == "unknown"]
    gates.append(
        {
            "id": "gallery_backward_facing",
            "ok": not backward,
            "detail": f"{len(backward)} models walk backwards (root motion -X), {len(unknown)} unknown",
            "models": [{"path": r.get("path"), "vec": r.get("facing_vec")} for r in backward][:40],
        }
    )

    # thumbnail pixel gates
    bad_black, bad_white, bad_pink, missing_thumbs = [], [], [], []
    for row in ok_models:
        thumb = gallery / (row.get("thumb") or "")
        if not thumb.exists():
            missing_thumbs.append(row.get("path"))
            continue
        stats = thumb_stats(thumb)
        if stats["luma"] < 9.0 or stats["black"] > 0.93:
            bad_black.append({"path": row.get("path"), "luma": round(stats["luma"], 1), "black": round(stats["black"], 2)})
        if stats["white"] > 0.60:
            bad_white.append({"path": row.get("path"), "white": round(stats["white"], 2)})
        if stats["pink"] > 0.01:
            bad_pink.append({"path": row.get("path"), "pink": round(stats["pink"], 3)})
    gates.append(
        {
            "id": "gallery_thumbnails_black",
            "ok": not bad_black,
            "detail": f"{len(bad_black)} model thumbnails almost entirely black",
            "models": bad_black[:40],
        }
    )
    gates.append(
        {
            "id": "gallery_thumbnails_white",
            "ok": not bad_white,
            "detail": f"{len(bad_white)} model thumbnails dominated by white/missing texture",
            "models": bad_white[:40],
        }
    )
    gates.append(
        {
            "id": "gallery_thumbnails_pink",
            "ok": not bad_pink,
            "detail": f"{len(bad_pink)} model thumbnails contain magenta/alpha texels",
            "models": bad_pink[:40],
        }
    )
    gates.append(
        {
            "id": "gallery_thumbnails_present",
            "ok": not missing_thumbs,
            "detail": f"{len(missing_thumbs)} thumbnails missing from disk",
            "models": missing_thumbs[:40],
        }
    )

    # icons
    icon_rows = [r for r in load_manifest(gallery / "icons" / "manifest.jsonl") if r.get("kind") == "icon"]
    icons_ok = [r for r in icon_rows if r.get("ok")]
    icon_missing = [r for r in icons_ok if not (gallery / (r.get("thumb") or "")).exists()]
    icon_blank = [r for r in icons_ok if r.get("transparent_px", 0) > (64 * 64) // 2]
    icon_pink = [r for r in icons_ok if r.get("pink_px", 0) > int(64 * 64 * 0.40)]
    icon_load_fail = [r for r in icon_rows if not r.get("ok")]
    gates.append(
        {
            "id": "gallery_icon_counts",
            "ok": len(icon_rows) == TOTAL_ICONS and not icon_load_fail,
            "detail": f"icons={len(icon_rows)}/{TOTAL_ICONS}, load_fail={len(icon_load_fail)}",
            "failing": [r.get("path") for r in icon_load_fail][:40],
        }
    )
    gates.append(
        {
            "id": "gallery_icon_content",
            "ok": not icon_missing and not icon_blank and not icon_pink,
            "detail": f"missing={len(icon_missing)} blank={len(icon_blank)} pink={len(icon_pink)}",
            "blank": [r.get("path") for r in icon_blank][:40],
            "pink": [r.get("path") for r in icon_pink][:40],
        }
    )

    # terrain
    terrain_rows = [r for r in load_manifest(gallery / "terrain" / "manifest.jsonl") if r.get("kind") == "terrain"]
    terrain_ok = [r for r in terrain_rows if r.get("ok")]
    terrain_fail = [r for r in terrain_rows if not r.get("ok")]
    terrain_missing = [r for r in terrain_ok if not (gallery / (r.get("thumb") or "")).exists()]
    terrain_black = [r for r in terrain_ok if r.get("black_px", 0) > int(256 * 256 * 0.60) and "nothing" not in (r.get("path") or "").lower()]
    terrain_white = [r for r in terrain_ok if r.get("white_px", 0) > (256 * 256) // 2]
    gates.append(
        {
            "id": "gallery_terrain_counts",
            "ok": len(terrain_rows) == TOTAL_TERRAIN and not terrain_fail,
            "detail": f"terrain_tiles={len(terrain_rows)}/{TOTAL_TERRAIN}, load_fail={len(terrain_fail)}",
            "failing": [r.get("path") for r in terrain_fail][:40],
        }
    )
    gates.append(
        {
            "id": "gallery_terrain_content",
            "ok": not terrain_missing and not terrain_black and not terrain_white,
            "detail": f"missing={len(terrain_missing)} black={len(terrain_black)} white={len(terrain_white)}",
            "black": [r.get("path") for r in terrain_black][:40],
            "white": [r.get("path") for r in terrain_white][:40],
        }
    )

    sheets: dict[str, list[str]] = {}
    if not gates_only:
        models_dir = gallery / "models"
        for category in sorted(MODEL_CATEGORIES):
            files = sorted((models_dir / category).glob("*.ppm"))
            if files:
                sheets[f"models_{category}"] = contact_sheets(files, gallery / "sheets", f"models-{category}", sheet_size)
        icon_files = sorted((gallery / "icons").glob("*.ppm"))
        if icon_files:
            sheets["icons"] = contact_sheets(icon_files, gallery / "sheets", "icons", 64)
        terrain_files = sorted((gallery / "terrain").glob("*.ppm"))
        if terrain_files:
            sheets["terrain"] = contact_sheets(terrain_files, gallery / "sheets", "terrain", 128, cols=6, rows=5)

    if not models_built:
        gates.insert(
            0,
            {
                "id": "gallery_models_not_built",
                "ok": False,
                "detail": "model thumbnails not built: Renderer::new panics on the WGSL `patch` keyword (terrain_render.rs:628); icons/terrain still graded",
            },
        )
    hard = [g for g in gates if not g["ok"]]
    report = {
        "ok": not hard,
        "partial": not models_built,
        "gallery": common.rel(gallery),
        "gates": gates,
        "fail_count": len(hard),
        "counts": {
            "models": len(model_rows),
            "icons": len(icon_rows),
            "terrain": len(terrain_rows),
            "model_categories": category_counts,
        },
        "summary": summary,
        "contact_sheets": sheets,
        "analyzed_at": common.now_iso(),
    }
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--gallery", type=Path, default=common.ANALYSIS / "gallery")
    parser.add_argument("--json-out", type=Path, default=None)
    parser.add_argument("--gates-only", action="store_true")
    parser.add_argument("--sheet-size", type=int, default=128)
    args = parser.parse_args(argv)
    out = args.json_out or (args.gallery / "gates.json")
    if not any((args.gallery / part).exists() for part in ("models", "icons", "terrain")):
        print(f"GALLERY not built: {common.rel(args.gallery)}")
        return 2
    report = analyze(args.gallery, args.gates_only, args.sheet_size)
    common.save_json(out, report)
    print(f"GALLERY {'PASS' if report['ok'] else 'FAIL'} fails={report['fail_count']} {report['counts']}")
    for gate in report["gates"]:
        if not gate["ok"]:
            print(f"  FAIL {gate['id']}: {gate['detail']}")
    if report["contact_sheets"]:
        total = sum(len(v) for v in report["contact_sheets"].values())
        print(f"  contact sheets: {total} under {common.rel(args.gallery / 'sheets')}")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
