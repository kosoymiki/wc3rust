#!/usr/bin/env python3
"""Diff Figma frame geometry against the game's real HUD layout dump.

Compares the rectangles extracted by ``tools/figma_pull.py`` with the output
of ``crates/wc3-game/examples/hud_layout.rs`` (device pixels), element by
element, and exits non-zero when any matched element is off by more than the
threshold (default 2 px).

The comparison is the point: hand-tuned offsets are replaced by "this element
is 4.2 px lower than the design", measured, not argued.

Example::

    python3 tools/figma_diff.py --run-layout
    python3 tools/figma_diff.py --layout analysis/figma/layout.dump.txt \\
        --nodes analysis/figma/KEY.nodes.json --orientation landscape
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import figma_pull as fp  # noqa: E402  (same directory, stdlib module)

DEFAULT_THRESHOLD = 2.0
SECTION_RE = re.compile(r"^== viewport (\d+)x(\d+)\s+orientation=(\w+)")
ROW_SPLIT_RE = re.compile(r"\s{2,}")
NUMBER_RE = re.compile(r"^-?\d+(?:\.\d+)?$")

# Layout element name -> normalised name variants that may occur in Figma.
DEFAULT_ALIASES: dict[str, list[str]] = {
    "console": ["console", "actionbar", "commandbar", "hudconsole", "bottomconsole", "consolebar"],
    "top strip": ["topstrip", "topbar", "tophud", "resourcebar", "resources", "topmetadata", "topstatus"],
    "card plate": ["cardplate", "commandcard", "commandpanel", "abilitypanel", "skillpanel", "spellpanel", "actionpanel"],
    "card slot": ["cardslot", "commandslot", "abilityslot", "skillbutton", "spellbutton", "commandbutton"],
    "groups pad": ["groupspad", "controlgroups", "controlgroup", "grouppad", "groupbar", "controlgroupbar"],
    "group plate": ["groupplate", "groupbutton", "controlgroupslot", "controlslot"],
    "minimap image": ["minimapimage", "minimap", "minimapframe", "minimappanel"],
    "portrait": ["portrait", "heroportrait", "heroportraitframe", "faceportrait"],
    "inventory panel": ["inventorypanel", "inventory", "itembar"],
    "inventory slot": ["inventoryslot", "itemslot", "inventorycell"],
    "crest": ["crest", "centercrest", "emblem", "centermedallion", "medallion"],
    "army band": ["armyband", "armybar", "army", "unitbar", "unitband"],
    "army slot": ["armyslot", "unitslot", "unitbutton"],
    "tab button": ["tabbutton", "tab", "menutab", "tabs"],
}

STRIPPABLE_SUFFIXES = ("bg", "background", "frame", "panel", "container", "group", "image", "plate", "area")


class DiffError(RuntimeError):
    """A setup problem: no frame, no matches, unparsable dump."""


def norm_key(name: str) -> str:
    return re.sub(r"[^a-z0-9]+", "", (name or "").lower())


def name_variants(name: str) -> set[str]:
    """Normalised names to index: full name and suffix-stripped variants."""
    base = norm_key(name)
    variants = {base} if base else set()
    current = base
    changed = True
    while changed:
        changed = False
        for suffix in STRIPPABLE_SUFFIXES:
            if current.endswith(suffix) and len(current) > len(suffix) + 2:
                current = current[: -len(suffix)]
                variants.add(current)
                changed = True
                break
    segments = [segment for segment in re.split(r"[/|]", name or "") if segment.strip()]
    if segments:
        variants.add(norm_key(segments[-1]))
    return {variant for variant in variants if variant}


def parse_layout(text: str) -> dict[str, dict]:
    """Parse the hud_layout.rs dump into {orientation: {element: rect}}."""
    sections: dict[str, dict] = {}
    current: dict | None = None
    for raw_line in text.splitlines():
        line = raw_line.strip()
        match = SECTION_RE.match(line)
        if match:
            current = {
                "w": int(match.group(1)),
                "h": int(match.group(2)),
                "orientation": match.group(3).lower(),
                "elements": {},
            }
            sections[current["orientation"]] = current
            continue
        if current is None or not line:
            continue
        parts = [part.strip() for part in ROW_SPLIT_RE.split(line)]
        if len(parts) < 5:
            continue
        values = parts[1:5]
        if not all(NUMBER_RE.match(value) for value in values):
            continue
        x, y, width, height = (float(value) for value in values)
        current["elements"][parts[0]] = {
            "x": x,
            "y": y,
            "w": width,
            "h": height,
            "verdict": parts[6] if len(parts) > 6 else "",
        }
    return sections


def run_layout_dump(repo_root: Path, timeout: int = 900) -> str:
    binary = repo_root / "target" / "debug" / "examples" / "hud_layout"
    if binary.is_file():
        command = [str(binary)]
    else:
        command = [
            str(repo_root / "scripts" / "h2-cargo"),
            "run",
            "-q",
            "-p",
            "wc3-game",
            "--example",
            "hud_layout",
            "--offline",
        ]
    try:
        result = subprocess.run(
            command,
            cwd=repo_root,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise DiffError(f"could not run hud_layout: {error}") from None
    if result.returncode != 0:
        tail = (result.stderr or result.stdout or "")[-1200:]
        raise DiffError(f"hud_layout exited {result.returncode}:\n{tail}")
    return result.stdout


def latest_nodes_file(out_dir: Path) -> Path:
    candidates = sorted(out_dir.glob("*.nodes.json"), key=lambda path: path.stat().st_mtime)
    if not candidates:
        raise DiffError(f"no *.nodes.json in {out_dir}; run tools/figma_pull.py first")
    return candidates[-1]


def choose_frame(
    extraction: dict, frame_arg: str | None, target_aspect: float
) -> dict:
    frames = [frame for frame in extraction["frames"] if frame.get("bbox")]
    if frame_arg:
        for frame in frames:
            if frame["id"] == frame_arg:
                return frame
        needle = frame_arg.lower()
        named = [frame for frame in frames if needle in frame["name"].lower()]
        if len(named) == 1:
            return named[0]
        if not named:
            raise DiffError(f"--frame {frame_arg!r} matches no frame")
        raise DiffError(
            f"--frame {frame_arg!r} is ambiguous: "
            + ", ".join(f"{frame['name']} ({frame['id']})" for frame in named[:6])
        )
    hud = fp.pick_hud_frame(extraction, target_aspect)
    if hud is None:
        raise DiffError("no candidate HUD frame in the extraction")
    return hud


def build_index(frames: list[dict]) -> dict[str, list[dict]]:
    index: dict[str, list[dict]] = {}
    for frame in frames:
        for variant in name_variants(frame["name"]):
            index.setdefault(variant, []).append(frame)
    return index


def pick_candidate(candidates: list[dict]) -> tuple[dict, str]:
    """Prefer the shallowest (outermost) node; then the largest."""
    ordered = sorted(
        candidates,
        key=lambda frame: (frame["depth"], -frame["bbox"]["w"] * frame["bbox"]["h"]),
    )
    chosen = ordered[0]
    note = "unique" if len(candidates) == 1 else f"auto-picked from {len(candidates)}"
    return chosen, note


def diff_section(
    section: dict,
    extraction: dict,
    frame: dict,
    aliases: dict[str, list[str]],
    explicit_map: dict[str, str],
    scale_override: float | None,
    threshold: float,
    anchor: str = "strict",
) -> dict:
    frame_box = frame["bbox"]
    view_w, view_h = section["w"], section["h"]
    if scale_override:
        scale_x = scale_y = float(scale_override)
    else:
        scale_x = view_w / frame_box["w"]
        scale_y = view_h / frame_box["h"]
        aspect_error = abs(scale_x - scale_y) / max(scale_x, 1e-9)
        if anchor == "strict" and aspect_error > 0.015:
            raise DiffError(
                f"frame {frame['name']!r} is {frame_box['w']:.0f}x{frame_box['h']:.0f} "
                f"but the layout viewport is {view_w}x{view_h} "
                f"(scale {scale_x:.4f} vs {scale_y:.4f}); pass a frame with the "
                f"viewport aspect, or --scale with --anchor topleft|bottomleft|center|auto"
            )
    within = [
        candidate
        for candidate in extraction["frames"]
        if candidate["id"] != frame["id"]
        and candidate.get("bbox")
        and candidate["path"].startswith(frame["path"] + "/")
    ]
    index = build_index(within)
    nodes_by_id = {node["id"]: node for node in extraction["all_nodes"]}

    def map_y(box: dict) -> float:
        local_y = box["y"] - frame_box["y"]
        if anchor == "bottomleft":
            return view_h - (frame_box["h"] - local_y - box["h"]) * scale_y - box["h"] * scale_y
        if anchor == "center":
            return (view_h - frame_box["h"] * scale_y) / 2.0 + local_y * scale_y
        if anchor == "auto":
            centre = local_y + box["h"] / 2.0
            if centre > frame_box["h"] / 2.0:
                return view_h - (frame_box["h"] - local_y - box["h"]) * scale_y - box["h"] * scale_y
        return local_y * scale_y

    rows: list[dict] = []
    missing: list[str] = []
    ambiguous: list[str] = []
    used: set[str] = set()
    for element, ours in section["elements"].items():
        candidates: list[dict] = []
        note = "unique"
        if element in explicit_map:
            node = nodes_by_id.get(explicit_map[element])
            if node and node.get("bbox"):
                candidates = [node]
            else:
                missing.append(f"{element} (map id {explicit_map[element]} not found)")
                continue
        else:
            keys: set[str] = set(aliases.get(element, []))
            keys.add(norm_key(element))
            for key in keys:
                candidates.extend(index.get(key, []))
            deduped = {candidate["id"]: candidate for candidate in candidates}
            candidates = list(deduped.values())
            if not candidates:
                missing.append(element)
                continue
            if len(candidates) > 1:
                ambiguous.append(f"{element}: {len(candidates)}")
        picked, note = pick_candidate(candidates)
        used.add(picked["id"])
        box = picked["bbox"]
        figma = {
            "x": (box["x"] - frame_box["x"]) * scale_x,
            "y": map_y(box),
            "w": box["w"] * scale_x,
            "h": box["h"] * scale_y,
        }
        deltas = {
            "x": figma["x"] - ours["x"],
            "y": figma["y"] - ours["y"],
            "w": figma["w"] - ours["w"],
            "h": figma["h"] - ours["h"],
        }
        worst = max(abs(value) for value in deltas.values())
        rows.append(
            {
                "element": element,
                "figma": figma,
                "ours": ours,
                "delta": deltas,
                "max_abs": worst,
                "status": "OK" if worst <= threshold else "FAIL",
                "node": {
                    "id": picked["id"],
                    "name": picked["name"],
                    "path": picked["path"],
                },
                "match": note,
            }
        )
    others = [
        candidate
        for candidate in within
        if candidate["id"] not in used
        and not any(
            candidate["id"] == row["node"]["id"] for row in rows
        )
    ]
    return {
        "frame": frame,
        "scale": {"x": scale_x, "y": scale_y},
        "rows": rows,
        "missing": missing,
        "ambiguous": ambiguous,
        "other_frames": len(others),
        "other_sample": [frame["name"] for frame in others[:12]],
        "threshold": threshold,
        "viewport": {"w": view_w, "h": view_h},
    }


def format_table(rows: list[dict]) -> str:
    header = (
        f"{'element':<16} {'figma x,y,w,h':>30} {'ours x,y,w,h':>30} "
        f"{'dx':>7} {'dy':>7} {'dw':>7} {'dh':>7} {'max':>6}  verdict"
    )
    lines = [header, "-" * len(header)]
    for row in rows:
        figma = row["figma"]
        ours = row["ours"]
        delta = row["delta"]
        lines.append(
            f"{row['element']:<16} "
            f"{figma['x']:>7.1f},{figma['y']:>7.1f},{figma['w']:>6.1f},{figma['h']:>6.1f} "
            f"{ours['x']:>7.0f},{ours['y']:>7.0f},{ours['w']:>6.1f},{ours['h']:>6.1f} "
            f"{delta['x']:>7.1f} {delta['y']:>7.1f} {delta['w']:>7.1f} {delta['h']:>7.1f} "
            f"{row['max_abs']:>6.1f}  {row['status']}"
        )
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Figma vs hud_layout geometry diff.")
    parser.add_argument("--nodes", help="extraction JSON from tools/figma_pull.py")
    parser.add_argument("--layout", help="hud_layout.rs text dump ('-' for stdin)")
    parser.add_argument("--run-layout", action="store_true", help="run the hud_layout example")
    parser.add_argument("--orientation", choices=("landscape", "portrait"), default="landscape")
    parser.add_argument("--frame", help="HUD frame id or unique name substring")
    parser.add_argument("--scale", type=float, help="base scale: design px -> device px")
    parser.add_argument(
        "--anchor",
        choices=("strict", "topleft", "bottomleft", "center", "auto"),
        default="strict",
        help="vertical mapping when the frame aspect differs from the viewport",
    )
    parser.add_argument("--threshold", type=float, default=DEFAULT_THRESHOLD)
    parser.add_argument("--map", help="JSON {layout_element: node_id} for explicit pairing")
    parser.add_argument("--out-dir", default=str(fp.DEFAULT_OUT))
    parser.add_argument("--md-out", help="Markdown report path")
    parser.add_argument("--json-out", help="JSON report path")
    parser.add_argument("--no-write", action="store_true", help="print only")
    args = parser.parse_args(argv)

    out_dir = Path(args.out_dir).expanduser()
    if args.anchor != "strict":
        print(
            f"warning: --anchor {args.anchor} is an approximate mapping for a frame "
            f"whose aspect differs from the viewport; verify the scale source",
            file=sys.stderr,
        )
    nodes_path = Path(args.nodes).expanduser() if args.nodes else latest_nodes_file(out_dir)
    extraction = json.loads(nodes_path.read_text(encoding="utf-8"))

    if args.layout:
        text = sys.stdin.read() if args.layout == "-" else Path(args.layout).expanduser().read_text()
        layout_source = "stdin" if args.layout == "-" else str(Path(args.layout).expanduser())
    elif args.run_layout:
        text = run_layout_dump(fp.REPO_ROOT)
        layout_source = "scripts/h2-cargo run -p wc3-game --example hud_layout --offline"
    else:
        parser.error("one of --layout or --run-layout is required")
    sections = parse_layout(text)
    section = sections.get(args.orientation)
    if not section:
        raise DiffError(
            f"layout dump has no {args.orientation} section "
            f"(found: {', '.join(sorted(sections)) or 'none'})"
        )

    target_aspect = section["w"] / section["h"]
    frame = choose_frame(extraction, args.frame, target_aspect)
    explicit_map = {}
    if args.map:
        explicit_map = json.loads(Path(args.map).expanduser().read_text(encoding="utf-8"))
    result = diff_section(
        section,
        extraction,
        frame,
        DEFAULT_ALIASES,
        explicit_map,
        args.scale,
        args.threshold,
        args.anchor,
    )

    rows = result["rows"]
    failures = [row for row in rows if row["status"] == "FAIL"]
    print(
        f"nodes: {nodes_path}\n"
        f"frame: {frame['name']!r} id={frame['id']} "
        f"{frame['bbox']['w']:.0f}x{frame['bbox']['h']:.0f} "
        f"path={frame['path']}\n"
        f"layout: {layout_source} "
        f"{section['w']}x{section['h']} scale={result['scale']['x']:.5f}\n"
        f"threshold: {args.threshold:.1f} px"
    )
    print(format_table(rows))
    print(
        f"matched={len(rows)} over_threshold={len(failures)} "
        f"missing={len(result['missing'])} ambiguous={len(result['ambiguous'])} "
        f"other_frames={result['other_frames']}"
    )
    if result["missing"]:
        print("missing in Figma: " + ", ".join(result["missing"]))
    if result["ambiguous"]:
        print("ambiguous matches: " + "; ".join(result["ambiguous"]))
    if result["other_frames"]:
        print(f"other frames not compared: {result['other_sample']}")

    if not rows:
        raise DiffError("no layout element matched a Figma node")

    generated = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    report = {
        "generated_at": generated,
        "nodes_path": str(nodes_path),
        "file_key": extraction.get("file_key"),
        "file_name": extraction.get("file_name"),
        "frame": {
            "id": frame["id"],
            "name": frame["name"],
            "path": frame["path"],
            "bbox": frame["bbox"],
        },
        "layout_source": layout_source,
        "orientation": args.orientation,
        "viewport": result["viewport"],
        "scale": result["scale"],
        "anchor": args.anchor,
        "threshold": args.threshold,
        "rows": rows,
        "missing": result["missing"],
        "ambiguous": result["ambiguous"],
        "other_frames": result["other_frames"],
    }
    if not args.no_write:
        md_path = (
            Path(args.md_out).expanduser()
            if args.md_out
            else out_dir / f"diff-{args.orientation}.md"
        )
        json_path = (
            Path(args.json_out).expanduser()
            if args.json_out
            else out_dir / f"diff-{args.orientation}.json"
        )
        lines = [
            f"# Figma vs hud_layout — {args.orientation}",
            "",
            f"- generated: {generated}",
            f"- file: `{extraction.get('file_name')}` key `{extraction.get('file_key')}`",
            f"- frame: `{frame['name']}` `{frame['id']}` "
            f"{frame['bbox']['w']:.0f}x{frame['bbox']['h']:.0f} (`{frame['path']}`)",
            f"- layout: `{layout_source}` viewport {section['w']}x{section['h']}",
            f"- scale: {result['scale']['x']:.5f} / {result['scale']['y']:.5f}",
            f"- threshold: {args.threshold:.1f} px",
            "",
            "```text",
            format_table(rows),
            "```",
            "",
            f"matched={len(rows)} over_threshold={len(failures)} "
            f"missing={len(result['missing'])} ambiguous={len(result['ambiguous'])}",
            "",
        ]
        if result["missing"]:
            lines.append("Missing in Figma: " + ", ".join(result["missing"]))
        if failures:
            lines.append("")
            lines.append("Over threshold:")
            for row in failures:
                lines.append(
                    f"- **{row['element']}** max {row['max_abs']:.1f} px "
                    f"(node `{row['node']['name']}` `{row['node']['id']}`, {row['match']})"
                )
        fp.atomic_write_text(md_path, "\n".join(lines) + "\n")
        fp.atomic_write_text(json_path, json.dumps(report, indent=2, ensure_ascii=False) + "\n")
        print(f"report: {md_path} + {json_path}")

    return 1 if failures else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except DiffError as error:
        print(f"figma_diff: {error}", file=sys.stderr)
        raise SystemExit(2)
