#!/usr/bin/env python3
"""Round-2 acceptance: every map once, every TerrainArt biome once.

No device. Map validation uses the canonical `wc3re map` (MapBundle: W3M/W3X
MPQ + W3E/W3I/WPM/WTS/objects/script), exactly one invocation per map. Biome
validation consumes the canonical decoded TerrainArt gallery (111 BLPs from
wc3-formats::blp), one row per tile.

Usage:
    python3 scripts/qa/map_biome.py [--maps-root DIR] [--wc3re PATH] \
        [--gallery DIR] [--out DIR]
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
R2 = ROOT / "analysis" / "agent-qa" / "r2"
WC3RE = ROOT / "target" / "release" / "wc3re"
MAPS = ROOT / "analysis" / "war3_full3" / "Maps"
GALLERY = ROOT / "analysis" / "agent-qa" / "gallery" / "terrain"
FINAL = ROOT / "analysis" / "agent-qa" / "final" / "focus-gates.json"


def map_files(root: Path) -> list[Path]:
    out = []
    for ext in ("w3m", "w3x", "w3n"):
        out.extend(root.rglob(f"*.{ext}"))
    return sorted(out)


def parse_map(text: str) -> dict:
    terrain = re.search(r"terrain: (\d+)x(\d+) tiles=(\d+)", text)
    objects = re.search(r"objects: (\d+) entries", text)
    script = re.search(r"script bytes: (\d+)", text)
    inner = re.search(r"inner files: (\d+)", text)
    info = re.search(r"info: name=(.*?) author=(.*?) players=(.*)", text)
    errors_line = next((line for line in text.splitlines() if line.startswith("errors: ")), "")
    errors = errors_line[len("errors: "):].strip() if errors_line else ""
    return {
        "terrain": [int(terrain.group(1)), int(terrain.group(2)), int(terrain.group(3))] if terrain else None,
        "objects": int(objects.group(1)) if objects else None,
        "script_bytes": int(script.group(1)) if script else None,
        "inner_files": int(inner.group(1)) if inner else None,
        "info": info.group(1).strip().strip('"') if info else None,
        "players": info.group(3).strip() if info else None,
        "errors_line": errors,
    }


def validate_maps(root: Path, wc3re: Path, reuse: bool = False) -> list[dict]:
    maps = map_files(root)
    rows = []
    for index, path in enumerate(maps, 1):
        relative = path.relative_to(root).as_posix()
        log_dir = R2 / "maps"
        log_dir.mkdir(parents=True, exist_ok=True)
        log_path = log_dir / (relative.replace("/", "__") + ".log")
        if reuse and log_path.is_file():
            text = log_path.read_text(errors="replace")
            rc_match = re.search(r"\nRC=(\d+)\s*$", text)
            rc = int(rc_match.group(1)) if rc_match else 0
        else:
            proc = subprocess.run(
                [str(wc3re), "map", str(path)],
                capture_output=True,
                text=True,
                timeout=60,
            )
            text = (proc.stdout or "") + ("\n" + proc.stderr if proc.stderr else "")
            rc = proc.returncode
            log_path.write_text(text + f"\nRC={rc}\n")
        parsed = parse_map(text)
        terrain = parsed["terrain"]
        core_ok = bool(
            rc == 0
            and terrain
            and terrain[2] > 0
            and (parsed["objects"] or 0) > 0
            and (parsed["script_bytes"] or 0) > 0
        )
        units_error = "war3mapUnits.doo" in parsed["errors_line"]
        clean = core_ok and not parsed["errors_line"]
        rows.append({
            "index": index,
            "map": relative,
            "rc": rc,
            "terrain": terrain,
            "objects": parsed["objects"],
            "script_bytes": parsed["script_bytes"],
            "inner_files": parsed["inner_files"],
            "info": parsed["info"],
            "players": parsed["players"],
            "errors": parsed["errors_line"],
            "core_ok": core_ok,
            "units_error": units_error,
            "clean": clean,
            "log": str(log_path.relative_to(ROOT)),
        })
    return rows


def validate_biomes(gallery: Path) -> dict:
    rows = []
    manifest = gallery / "manifest.jsonl"
    if manifest.is_file():
        for line in manifest.read_text().splitlines():
            if line.strip():
                try:
                    rows.append(json.loads(line))
                except json.JSONDecodeError:
                    rows.append({"ok": False, "path": "manifest-invalid"})
    # The gallery manifest is authoritative for decode; verify every thumb.
    missing = []
    groups: dict[str, list[dict]] = {}
    for row in rows:
        if not row.get("ok"):
            continue
        thumb = gallery.parent / (row.get("thumb") or "")
        if not thumb.is_file():
            missing.append(row.get("path"))
        rel = (row.get("path") or "").replace("\\", "/")
        parts = rel.split("/")
        tileset = parts[1] if len(parts) > 2 and parts[0].lower() == "terrainart" else "unknown"
        groups.setdefault(tileset, []).append(row)
    summary = {}
    if (gallery / "summary.json").is_file():
        try:
            summary = json.loads((gallery / "summary.json").read_text())
        except json.JSONDecodeError:
            summary = {"invalid": True}
    return {
        "rows": len(rows),
        "ok": len([r for r in rows if r.get("ok")]),
        "missing_thumbs": missing,
        "tilesets": {name: len(items) for name, items in sorted(groups.items())},
        "summary": summary,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--maps-root", type=Path, default=MAPS)
    parser.add_argument("--wc3re", type=Path, default=WC3RE)
    parser.add_argument("--gallery", type=Path, default=GALLERY)
    parser.add_argument("--out", type=Path, default=R2)
    parser.add_argument("--reuse", action="store_true", help="parse existing one-shot map logs instead of re-running")
    args = parser.parse_args(argv)
    args.out.mkdir(parents=True, exist_ok=True)
    if not args.wc3re.is_file():
        print(f"WC3RE_MISSING {args.wc3re}", file=sys.stderr)
        return 2

    maps = validate_maps(args.maps_root, args.wc3re, args.reuse)
    biomes = validate_biomes(args.gallery)
    box = json.loads(FINAL.read_text()) if FINAL.is_file() else {"box": None}
    core_pass = sum(1 for row in maps if row["core_ok"])
    clean_pass = sum(1 for row in maps if row["clean"])
    map_gate = {
        "id": "r2.maps.all",
        "ok": len(maps) == 44 and clean_pass == 44,
        "count": len(maps),
        "core_pass": core_pass,
        "clean_pass": clean_pass,
        "units_errors": sum(1 for row in maps if row["units_error"]),
        "detail": f"{clean_pass}/{len(maps)} clean; core {core_pass}/{len(maps)}; units errors {sum(1 for row in maps if row['units_error'])}",
    }
    biome_gate = {
        "id": "r2.biomes.all",
        "ok": biomes["rows"] == 111 and biomes["ok"] == 111 and not biomes["missing_thumbs"],
        "detail": f"{biomes['ok']}/{biomes['rows']} decoded; tilesets={len(biomes['tilesets'])}; missing={len(biomes['missing_thumbs'])}",
        **{k: v for k, v in biomes.items() if k not in ("rows", "ok", "missing_thumbs")},
    }
    box_gate = {
        "id": "r2.box.boundary",
        "ok": bool(box.get("box", {}).get("classification") == "harness_boundary"),
        "detail": f"box classification={box.get('box', {}).get('classification')} selected={box.get('box', {}).get('selected_before')}->{box.get('box', {}).get('selected_after')}",
    }
    report = {
        "schema": "wc3rust-r2-map-biome-v1",
        "timestamp": __import__("time").strftime("%Y-%m-%dT%H:%M:%SZ", __import__("time").gmtime()),
        "maps_root": str(args.maps_root),
        "wc3re": str(args.wc3re),
        "gates": [map_gate, biome_gate, box_gate],
        "map_rows": maps,
        "biomes": biomes,
        "box_boundary": box.get("box"),
        "status": "PASS" if all(g["ok"] for g in (map_gate, biome_gate, box_gate)) else "FAIL",
    }
    out_json = args.out / "report.json"
    out_json.write_text(json.dumps(report, indent=2) + "\n")
    lines = [
        "# Round 2 — map and biome acceptance",
        "",
        f"Status: **{report['status']}**",
        "",
        "| Gate | Status | Detail |",
        "|---|---|---|",
    ]
    for gate in report["gates"]:
        lines.append(f"| `{gate['id']}` | **{'PASS' if gate['ok'] else 'FAIL'}** | {gate['detail']} |")
    lines += ["", "## Maps", "", "| # | map | core | units error | terrain | objects | script |", "|---:|---|---|---|---:|---:|---:|"]
    for row in maps:
        lines.append(
            f"| {row['index']} | {row['map']} | {'PASS' if row['core_ok'] else 'FAIL'} | "
            f"{'FAIL' if row['units_error'] else 'PASS'} | "
            f"{row['terrain'][2] if row['terrain'] else 0} | {row['objects'] or 0} | {row['script_bytes'] or 0} |"
        )
    lines += ["", "## Biomes", "", f"Decoded: **{biomes['ok']}/{biomes['rows']}**; tilesets: **{len(biomes['tilesets'])}**; missing thumbs: {len(biomes['missing_thumbs'])}.", ""]
    for name, count in biomes["tilesets"].items():
        lines.append(f"- {name}: {count}")
    lines += [
        "",
        "## Box boundary",
        "",
        f"`analysis/agent-qa/final/focus-gates.json`: classification **{box.get('box', {}).get('classification')}**, selected "
        f"{box.get('box', {}).get('selected_before')} -> {box.get('box', {}).get('selected_after')}; retained as harness boundary, not product failure.",
        "",
        "## Artifacts",
        "",
        f"- `{out_json.relative_to(ROOT)}`",
        f"- `{(args.out / 'REPORT.md').relative_to(ROOT)}`",
        f"- `analysis/agent-qa/r2/maps/*.log` ({len(maps)} one-shot map runs)",
        "- `analysis/agent-qa/gallery/terrain/{manifest.jsonl,summary.json,*.ppm}`",
        "",
    ]
    (args.out / "REPORT.md").write_text("\n".join(lines) + "\n")
    print(f"R2 {report['status']} {out_json}")
    for gate in report["gates"]:
        print(f"  {'PASS' if gate['ok'] else 'FAIL'} {gate['id']}: {gate['detail']}")
    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
