#!/usr/bin/env python3
"""Figma -> HUD pack pipeline.

Pulls one Figma file through the REST API, extracts every FRAME/COMPONENT/
GROUP (geometry, auto-layout, fills, strokes, corner radius) and every TEXT
node, exports the requested nodes as PNG, and emits a manifest in the same
schema as the operator's HUD pack so ``hudskin/pack.rs`` can load it.

Token: ``FIGMA_TOKEN`` in the environment (the operator's personal access
token; ``X-Figma-Token`` header). The token is never written to disk, never
logged, and never sent to the S3 URLs that ``/v1/images`` returns.

Endpoints (see analysis/figma/README.md for rate limits and provenance):

    GET /v1/me
    GET /v1/files/{key}?depth=N
    GET /v1/images/{key}?ids=...&format=png&scale=S&use_absolute_bounds=true

Typical use::

    set -a; . ~/.config/h2/figma.env; set +a
    python3 tools/figma_pull.py --file-url 'https://www.figma.com/design/KEY/Name' --list
    python3 tools/figma_pull.py --file KEY --nodes 1:2,3:4 --scale 2 --stage-pack
    python3 tools/figma_pull.py --dry-run --file KEY --input analysis/figma/KEY.json

Exit codes: 0 ok, 1 API/validation error, 2 usage, 3 token missing.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import struct
import sys
import time
from pathlib import Path
from urllib import error as urlerror
from urllib import parse as urlparse
from urllib import request as urlrequest

API = "https://api.figma.com"
USER_AGENT = "wc3rust-figma-pull/1.0 (Termux/Android; wc3rust)"
REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUT = REPO_ROOT / "analysis" / "figma"
CANONICAL_PACK = (
    REPO_ROOT / "analysis" / "hudskin" / "final" / "wc3rust-hud-final"
)
CANONICAL_MANIFEST = CANONICAL_PACK / "manifest.json"
DESIGN_HEIGHT = 768.0
MAX_INSETS_SHARE = 0.5  # insets may never exceed half of the shorter side

FRAME_TYPES = {
    "FRAME",
    "COMPONENT",
    "COMPONENT_SET",
    "INSTANCE",
    "GROUP",
    "SECTION",
}
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
FILE_URL_RE = re.compile(
    r"figma\.com/(?:file|design|proto|board|slides|community/file)/([A-Za-z0-9]{8,64})"
)
NODE_ID_RE = re.compile(r"[?&]node-id=([0-9]+[-:][0-9]+)")


class FigmaError(RuntimeError):
    """A pipeline failure with a message that is safe to print."""


# --------------------------------------------------------------------------
# HTTP
# --------------------------------------------------------------------------


def load_token(env_file: str | None) -> str:
    """Token from the environment, else from a KEY=VALUE env file."""
    token = os.environ.get("FIGMA_TOKEN", "").strip()
    if token:
        return token
    if env_file:
        path = Path(env_file).expanduser()
        if path.is_file():
            mode = path.stat().st_mode
            if mode & 0o077:
                print(
                    f"warning: {path} is readable by group/others; chmod 600 it",
                    file=sys.stderr,
                )
            for line in path.read_text(encoding="utf-8").splitlines():
                line = line.strip()
                if not line or line.startswith("#") or "=" not in line:
                    continue
                name, value = line.split("=", 1)
                if name.strip() == "FIGMA_TOKEN":
                    value = value.strip().strip("\"'")
                    if value:
                        return value
    raise FigmaError(
        "FIGMA_TOKEN is not set. Run: set -a; . ~/.config/h2/figma.env; set +a"
    )


def _request(url: str, token: str | None, timeout: int = 120) -> bytes:
    headers = {"Accept": "application/json", "User-Agent": USER_AGENT}
    if token:
        headers["X-Figma-Token"] = token
    request = urlrequest.Request(url, headers=headers)
    with urlrequest.urlopen(request, timeout=timeout) as response:
        return response.read()


def http_json(
    path: str,
    token: str,
    params: dict[str, str] | None = None,
    retries: int = 4,
    timeout: int = 120,
) -> dict:
    """GET JSON with 429/Retry-After and transient-network retries."""
    url = API + path
    if params:
        url += "?" + urlparse.urlencode(params)
    for attempt in range(retries):
        try:
            body = _request(url, token, timeout=timeout)
            return json.loads(body.decode("utf-8"))
        except urlerror.HTTPError as error:
            detail = error.read().decode("utf-8", "replace")[:400]
            if error.code == 429 and attempt + 1 < retries:
                retry_after = error.headers.get("Retry-After") or "1"
                try:
                    wait = min(60.0, max(1.0, float(retry_after)))
                except ValueError:
                    wait = 2.0
                print(
                    f"rate limited (HTTP 429), retrying in {wait:.0f}s "
                    f"(attempt {attempt + 1}/{retries})",
                    file=sys.stderr,
                )
                time.sleep(wait)
                continue
            raise FigmaError(f"HTTP {error.code} for {path}: {detail}") from None
        except urlerror.URLError as error:
            if attempt + 1 < retries:
                time.sleep(1.5**attempt)
                continue
            raise FigmaError(f"network error for {path}: {error.reason}") from None
    raise FigmaError(f"giving up after {retries} attempts: {path}")


def http_bytes(url: str, retries: int = 3, timeout: int = 180) -> bytes:
    """Download a pre-signed URL. No token: it would leak to S3."""
    for attempt in range(retries):
        try:
            return _request(url, None, timeout=timeout)
        except urlerror.HTTPError as error:
            if error.code in (429, 500, 502, 503, 504) and attempt + 1 < retries:
                time.sleep(2.0 * (attempt + 1))
                continue
            raise FigmaError(f"image download failed: HTTP {error.code}") from None
        except urlerror.URLError as error:
            if attempt + 1 < retries:
                time.sleep(1.5**attempt)
                continue
            raise FigmaError(f"image download failed: {error.reason}") from None
    raise FigmaError("image download failed after retries")


# --------------------------------------------------------------------------
# URL / key parsing
# --------------------------------------------------------------------------


def parse_file_url(url: str) -> tuple[str, list[str]]:
    match = FILE_URL_RE.search(url)
    if not match:
        raise FigmaError(
            f"not a Figma file URL (expected .../design/<key>/...): {url}"
        )
    key = match.group(1)
    node = NODE_ID_RE.search(url)
    node_ids = [node.group(1).replace("-", ":")] if node else []
    return key, node_ids


# --------------------------------------------------------------------------
# JSON helpers
# --------------------------------------------------------------------------


def atomic_write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(text, encoding="utf-8")
    os.replace(tmp, path)


def atomic_write_bytes(path: Path, blob: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_bytes(blob)
    os.replace(tmp, path)


def png_size(blob: bytes) -> tuple[int, int]:
    if len(blob) < 24 or not blob.startswith(PNG_MAGIC):
        raise FigmaError("downloaded payload is not a PNG")
    width, height = struct.unpack(">II", blob[16:24])
    if width <= 0 or height <= 0:
        raise FigmaError(f"PNG has an invalid size {width}x{height}")
    return width, height


def slug(text: str) -> str:
    cleaned = re.sub(r"[^a-zA-Z0-9]+", "_", text or "").strip("_").lower()
    return cleaned or "node"


# --------------------------------------------------------------------------
# Fetch + extract
# --------------------------------------------------------------------------


def fetch_file(key: str, token: str, depth: int) -> dict:
    params: dict[str, str] = {}
    if depth and depth > 0:
        params["depth"] = str(depth)
    return http_json(f"/v1/files/{urlparse.quote(key)}", token, params)


def _color(value: dict | None) -> dict:
    value = value or {}
    return {
        channel: round(float(value.get(channel, 0.0)), 6)
        for channel in ("r", "g", "b", "a")
    }


def _paint_list(paints) -> list[dict]:
    result: list[dict] = []
    for paint in paints or []:
        if not isinstance(paint, dict):
            continue
        entry: dict = {"type": paint.get("type")}
        if paint.get("visible") is False:
            entry["visible"] = False
        if paint.get("type") == "SOLID":
            entry["color"] = _color(paint.get("color"))
        if isinstance(paint.get("opacity"), (int, float)):
            entry["opacity"] = round(float(paint["opacity"]), 6)
        if paint.get("blendMode"):
            entry["blendMode"] = paint["blendMode"]
        result.append(entry)
    return result


def _box(node: dict) -> dict | None:
    for field in ("absoluteBoundingBox", "absoluteRenderBounds"):
        box = node.get(field)
        if not isinstance(box, dict):
            continue
        try:
            x = float(box["x"])
            y = float(box["y"])
            width = float(box["width"])
            height = float(box["height"])
        except (KeyError, TypeError, ValueError):
            continue
        return {"x": x, "y": y, "w": width, "h": height}
    return None


PADDING_FIELDS = (
    "paddingLeft",
    "paddingTop",
    "paddingRight",
    "paddingBottom",
)


def _node_record(node: dict, path: str, depth: int, parent: str | None, page: str) -> dict:
    node_type = node.get("type", "UNKNOWN")
    record: dict = {
        "id": node.get("id"),
        "name": node.get("name", ""),
        "type": node_type,
        "path": path,
        "depth": depth,
        "parent": parent,
        "page": page,
        "visible": node.get("visible", True),
        "bbox": _box(node),
    }
    if node_type in FRAME_TYPES:
        padding = {
            key: node[key] for key in PADDING_FIELDS if isinstance(node.get(key), (int, float))
        }
        record.update(
            {
                "layoutMode": node.get("layoutMode"),
                "itemSpacing": node.get("itemSpacing"),
                "padding": padding,
                "primaryAxisAlignItems": node.get("primaryAxisAlignItems"),
                "counterAxisAlignItems": node.get("counterAxisAlignItems"),
                "layoutSizingHorizontal": node.get("layoutSizingHorizontal"),
                "layoutSizingVertical": node.get("layoutSizingVertical"),
                "clipsContent": node.get("clipsContent"),
                "opacity": node.get("opacity", 1.0),
                "fills": _paint_list(node.get("fills")),
                "strokes": _paint_list(node.get("strokes")),
                "strokeWeight": node.get("strokeWeight"),
                "strokeAlign": node.get("strokeAlign"),
                "cornerRadius": node.get("cornerRadius"),
                "rectangleCornerRadii": node.get("rectangleCornerRadii"),
                "children": len(node.get("children") or []),
            }
        )
    elif node_type == "TEXT":
        style = node.get("style") or {}
        record.update(
            {
                "characters": node.get("characters", ""),
                "fontSize": style.get("fontSize", node.get("fontSize")),
                "fontName": style.get("fontName", node.get("fontName")),
                "fontWeight": style.get("fontWeight"),
                "lineHeight": style.get("lineHeight", node.get("lineHeight")),
                "letterSpacing": style.get("letterSpacing", node.get("letterSpacing")),
                "textAlignHorizontal": style.get(
                    "textAlignHorizontal", node.get("textAlignHorizontal")
                ),
                "textAlignVertical": style.get(
                    "textAlignVertical", node.get("textAlignVertical")
                ),
                "textAutoResize": node.get("textAutoResize"),
                "fills": _paint_list(node.get("fills")),
                "opacity": node.get("opacity", 1.0),
            }
        )
    return record


def extract(raw: dict, file_key: str, fetched_at: str | None = None) -> dict:
    """Flatten the document tree into frames + texts + a per-node record."""
    document = raw.get("document") or {}
    if not isinstance(document, dict):
        raise FigmaError("file JSON has no document object")
    frames: list[dict] = []
    texts: list[dict] = []
    all_nodes: list[dict] = []
    stats = {
        "pages": 0,
        "nodes": 0,
        "frames": 0,
        "components": 0,
        "instances": 0,
        "groups": 0,
        "texts": 0,
        "invisible": 0,
    }

    def walk(node: dict, parent_path: str, depth: int, parent: str | None, page: str) -> None:
        name = node.get("name") or "?"
        path = f"{parent_path}/{name}" if parent_path else name
        record = _node_record(node, path, depth, parent, page)
        all_nodes.append(record)
        stats["nodes"] += 1
        node_type = record["type"]
        if record["visible"] is False:
            stats["invisible"] += 1
        if node_type in FRAME_TYPES:
            frames.append(record)
            stats["frames"] += 1
            if node_type == "COMPONENT":
                stats["components"] += 1
            elif node_type == "INSTANCE":
                stats["instances"] += 1
            elif node_type == "GROUP":
                stats["groups"] += 1
        elif node_type == "TEXT":
            texts.append(record)
            stats["texts"] += 1
        for child in node.get("children") or []:
            if isinstance(child, dict):
                walk(child, path, depth + 1, record["id"], page)

    for page in document.get("children") or []:
        if not isinstance(page, dict):
            continue
        stats["pages"] += 1
        walk(page, "", 1, document.get("id"), page.get("id", ""))

    return {
        "schema": "wc3rust.figma.nodes/1",
        "file_key": file_key,
        "file_name": raw.get("name"),
        "last_modified": raw.get("lastModified"),
        "version": raw.get("version"),
        "fetched_at": fetched_at,
        "document": {
            "id": document.get("id"),
            "name": document.get("name"),
            "type": document.get("type"),
        },
        "stats": stats,
        "frames": frames,
        "texts": texts,
        "all_nodes": all_nodes,
    }


# --------------------------------------------------------------------------
# Image export
# --------------------------------------------------------------------------


def export_images(
    key: str,
    node_ids: list[str],
    scale: float,
    token: str,
    out_dir: Path,
    nodes_by_id: dict[str, dict],
) -> dict[str, dict]:
    response = http_json(
        f"/v1/images/{urlparse.quote(key)}",
        token,
        {
            "ids": ",".join(node_ids),
            "format": "png",
            "scale": _scale_str(scale),
            "use_absolute_bounds": "true",
        },
    )
    images = response.get("images") or {}
    errors = response.get("err")
    exports: dict[str, dict] = {}
    png_dir = out_dir / "png"
    for node_id in node_ids:
        url = images.get(node_id)
        node = nodes_by_id.get(node_id, {})
        name = node.get("name") or node_id
        if not url:
            print(f"warning: no image for node {node_id} ({name})", file=sys.stderr)
            continue
        blob = http_bytes(url)
        width, height = png_size(blob)
        filename = f"{slug(name)}-{node_id.replace(':', '-')}.png"
        atomic_write_bytes(png_dir / filename, blob)
        exports[node_id] = {
            "file": f"png/{filename}",
            "w": width,
            "h": height,
            "scale": scale,
            "name": name,
            "bytes": len(blob),
        }
    if errors:
        print(f"warning: /v1/images reported errors: {errors}", file=sys.stderr)
    if not exports:
        raise FigmaError("no PNG was exported; check --nodes ids")
    return exports


def _scale_str(scale: float) -> str:
    return str(int(scale)) if float(scale).is_integer() else f"{scale:g}"


# --------------------------------------------------------------------------
# Manifest
# --------------------------------------------------------------------------


def load_canonical() -> dict | None:
    if not CANONICAL_MANIFEST.is_file():
        return None
    return json.loads(CANONICAL_MANIFEST.read_text(encoding="utf-8"))


def _aspect_ok(box: dict, target: float, tolerance: float = 0.02) -> bool:
    if box["h"] <= 0:
        return False
    return abs(box["w"] / box["h"] - target) <= target * tolerance


def pick_hud_frame(extraction: dict, target_aspect: float = 16 / 9) -> dict | None:
    """The operator's HUD frame: a top-level, 16:9/9:16, biggest candidate."""
    frames = [f for f in extraction["frames"] if f.get("bbox") and f["depth"] <= 3]
    if not frames:
        return None
    landscape = [f for f in frames if _aspect_ok(f["bbox"], target_aspect)]
    portrait = [f for f in frames if _aspect_ok(f["bbox"], 1 / target_aspect)]
    pool = landscape or portrait or frames
    named = [
        f
        for f in pool
        if re.search(r"hud|game|ui|layout|screen|console", f["name"], re.IGNORECASE)
    ]
    pool = named or pool
    return max(pool, key=lambda f: f["bbox"]["w"] * f["bbox"]["h"])


def _median(values: list[float]) -> float:
    ordered = sorted(values)
    mid = len(ordered) // 2
    if len(ordered) % 2:
        return ordered[mid]
    return (ordered[mid - 1] + ordered[mid]) / 2.0


def _cluster(values: list[float], tolerance: float = 2.0) -> list[float]:
    clusters: list[list[float]] = []
    for value in sorted(values):
        if clusters and abs(value - _median(clusters[-1])) <= tolerance:
            clusters[-1].append(value)
        else:
            clusters.append([value])
    return [_median(cluster) for cluster in clusters]


def detect_grid(children: list[dict]) -> dict | None:
    """4x3 (or 3x4) grid from child boxes: columns, rows, cell, gap."""
    boxes = [child["bbox"] for child in children if child.get("bbox")]
    if len(boxes) < 12:
        return None
    xs = _cluster([box["x"] for box in boxes])
    ys = _cluster([box["y"] for box in boxes])
    if len(xs) * len(ys) < len(boxes):
        return None
    columns, rows = len(xs), len(ys)
    if (columns, rows) not in ((4, 3), (3, 4)):
        return None
    widths = [box["w"] for box in boxes]
    heights = [box["h"] for box in boxes]
    cell_w, cell_h = _median(widths), _median(heights)
    dx = [b - a for a, b in zip(xs, xs[1:])]
    dy = [b - a for a, b in zip(ys, ys[1:])]
    gap_x = min(dx) - cell_w if dx else 0.0
    gap_y = min(dy) - cell_h if dy else 0.0
    gaps = [value for value in (gap_x, gap_y) if value > -1.0]
    gap = _median(gaps) if gaps else 0.0
    return {
        "columns": columns,
        "rows": rows,
        "cell": [round(cell_w, 3), round(cell_h, 3)],
        "gap": round(max(0.0, gap), 3),
        "order": "row-major",
    }


def find_named(frames: list[dict], pattern: str) -> list[dict]:
    regex = re.compile(pattern, re.IGNORECASE)
    return [frame for frame in frames if regex.search(frame["name"])]


def _children_of(extraction: dict, node_id: str) -> list[dict]:
    return [f for f in extraction["frames"] if f.get("parent") == node_id]


def detect_safe_area(extraction: dict, hud: dict) -> tuple[list[float], str] | None:
    candidates = find_named(extraction["frames"], r"safe.?area|margins?|content.?area")
    hud_box = hud["bbox"]
    for candidate in candidates:
        box = candidate.get("bbox")
        if not box or not (
            hud_box["x"] <= box["x"]
            and hud_box["y"] <= box["y"]
            and box["x"] + box["w"] <= hud_box["x"] + hud_box["w"] + 1
            and box["y"] + box["h"] <= hud_box["y"] + hud_box["h"] + 1
        ):
            continue
        insets = [
            box["x"] - hud_box["x"],
            box["y"] - hud_box["y"],
            (hud_box["x"] + hud_box["w"]) - (box["x"] + box["w"]),
            (hud_box["y"] + hud_box["h"]) - (box["y"] + box["h"]),
        ]
        return [round(max(0.0, value), 2) for value in insets], f"figma:{candidate['id']}"
    return None


def derive_insets(
    node: dict, png_w: int, png_h: int, scale: float
) -> tuple[list[int] | None, str]:
    """9-slice insets [l,t,r,b] in texture pixels from corners + stroke."""
    if node.get("type") == "TEXT":
        return [0, 0, 0, 0], "text"
    radii = node.get("rectangleCornerRadii")
    corner = node.get("cornerRadius")
    if isinstance(radii, list) and len(radii) == 4 and all(
        isinstance(value, (int, float)) for value in radii
    ):
        corners = [float(value) for value in radii]  # tl, tr, br, bl
    elif isinstance(corner, (int, float)):
        corners = [float(corner)] * 4
    elif corner == "mixed":
        return None, "manual:mixed-corners"
    else:
        return None, "manual:absent-cornerRadius"
    stroke = node.get("strokeWeight")
    stroke = float(stroke) if isinstance(stroke, (int, float)) else 0.0
    grown = [corner_value + stroke for corner_value in corners]
    if max(grown) <= 0:
        return [0, 0, 0, 0], "derived:square"
    insets = [
        max(grown[0], grown[3]),  # left: top-left, bottom-left
        max(grown[0], grown[1]),  # top: top-left, top-right
        max(grown[1], grown[2]),  # right: top-right, bottom-right
        max(grown[2], grown[3]),  # bottom: bottom-right, bottom-left
    ]
    scaled = [int(math.ceil(value * scale - 1e-9)) for value in insets]
    limit = max(0, int(math.floor(min(png_w, png_h) * MAX_INSETS_SHARE)) - 1)
    if limit <= 0:
        return None, "manual:insets-exceed-texture"
    if max(scaled) > limit:
        return [min(value, limit) for value in scaled], "derived:clamped"
    if max(corners) <= 0:
        return scaled, "derived:stroke-only"
    return scaled, "derived:cornerRadius+strokeWeight"


def build_manifest(
    extraction: dict,
    exports: dict[str, dict],
    canonical: dict | None,
    scale: float,
    out_dir: Path,
    insets_overrides: dict[str, list[float]] | None = None,
) -> dict:
    """HUD-pack-schema manifest from the extracted frames + exported PNGs."""
    if canonical is None:
        raise FigmaError(
            f"canonical HUD pack not found at {CANONICAL_MANIFEST}; "
            "command_card / control_groups / fonts cannot be sourced"
        )
    hud = pick_hud_frame(extraction)
    if hud is None:
        raise FigmaError("no frame with an absoluteBoundingBox in the file")
    hud_box = hud["bbox"]
    frame_w, frame_h = round(hud_box["w"]), round(hud_box["h"])
    if frame_w <= 0 or frame_h <= 0:
        raise FigmaError("HUD frame has an empty bounding box")
    landscape = [frame_w, frame_h] if frame_w >= frame_h else [frame_h, frame_w]
    portrait = [landscape[1], landscape[0]]

    frames = extraction["frames"]
    within = [f for f in frames if f["path"].startswith(hud["path"] + "/")]

    landscape_safe = list(canonical["safe_area_1080p"]["landscape"])
    portrait_safe = list(canonical["safe_area_1080p"]["portrait"])
    detected = detect_safe_area(extraction, hud)
    safe_source = "hudpack:canonical"
    if detected:
        safe_insets, _detected_source = detected
        scale_to_1080 = 1080.0 / (frame_h if frame_w >= frame_h else frame_w)
        safe_insets = [round(value * scale_to_1080, 2) for value in safe_insets]
        if frame_w >= frame_h:
            landscape_safe = safe_insets
            safe_source = f"figma:landscape:{hud['id']}"
        else:
            portrait_safe = safe_insets
            safe_source = f"figma:portrait:{hud['id']}"

    card_frame = None
    for candidate in find_named(within, r"command.?card|command.?panel|card.?plate|ability|skill|action.?panel"):
        grid = detect_grid(_children_of(extraction, candidate["id"]))
        if grid:
            card_frame = (candidate, grid)
            break
    if card_frame:
        command_card = card_frame[1]
        command_card_source = f"figma:{card_frame[0]['id']}"
    else:
        command_card = canonical["command_card"]
        command_card_source = "hudpack:canonical"

    groups_frame = None
    for candidate in find_named(within, r"control.?groups?|groups?.?pad|group.?pad|groups?.?bar"):
        kids = _children_of(extraction, candidate["id"])
        boxes = [kid["bbox"] for kid in kids if kid.get("bbox")]
        if len(boxes) >= 10:
            groups_frame = (candidate, boxes)
            break
    if groups_frame:
        cell_w = _median([box["w"] for box in groups_frame[1]])
        cell_h = _median([box["h"] for box in groups_frame[1]])
        control_groups = {
            "count": min(10, len(groups_frame[1])),
            "cell": [round(cell_w, 3), round(cell_h, 3)],
        }
        control_groups_source = f"figma:{groups_frame[0]['id']}"
    else:
        control_groups = canonical["control_groups"]
        control_groups_source = "hudpack:canonical"

    overrides = insets_overrides or {}
    nodes_by_id = {node["id"]: node for node in extraction["all_nodes"]}
    assets: list[dict] = []
    manual: list[dict] = []
    names_seen: dict[str, int] = {}
    for node_id, export in exports.items():
        node = nodes_by_id.get(node_id)
        if node is None:
            manual.append(
                {
                    "id": node_id,
                    "node_id": node_id,
                    "name": export.get("name"),
                    "insets_source": "manual:unknown-node",
                    "reason": "node not in file JSON",
                }
            )
            continue
        if node_id in overrides:
            insets = [round(float(value), 3) for value in overrides[node_id]]
            source = "override"
        else:
            insets, source = derive_insets(
                node, int(export["w"]), int(export["h"]), float(export.get("scale", scale))
            )
        if insets is None:
            manual.append(
                {
                    "id": node_id,
                    "node_id": node_id,
                    "name": node["name"],
                    "insets_source": source,
                    "reason": source,
                }
            )
            continue
        base_name = slug(node["name"])
        names_seen[base_name] = names_seen.get(base_name, 0) + 1
        name = base_name if names_seen[base_name] == 1 else f"{base_name}_{names_seen[base_name]}"
        assets.append(
            {
                "name": name,
                "file": export["file"],
                "w": int(export["w"]),
                "h": int(export["h"]),
                "insets": {
                    "l": insets[0],
                    "t": insets[1],
                    "r": insets[2],
                    "b": insets[3],
                },
                "insets_source": source,
                "states": [],
                "atlas": "standalone",
                "mips": False,
                "format": "PNG RGBA8 sRGB straight-alpha",
                "node_id": node_id,
                "node_name": node["name"],
                "design": node.get("bbox"),
            }
        )

    if not assets:
        raise FigmaError(
            "no asset survived insets derivation; pass --insets-overrides or --assume-zero-radius"
        )

    canonical_atlas = canonical["atlas"]["commandbuttons"]
    atlas = {
        "commandbuttons": {
            "columns": canonical_atlas["columns"],
            "rows": canonical_atlas["rows"],
            "cell": canonical_atlas["cell"],
        }
    }

    manifest = {
        "schema_version": 1,
        "project": "wc3rust",
        "kind": "figma-design",
        "reference_resolution": {
            "landscape": landscape,
            "portrait": portrait,
        },
        "design_height": DESIGN_HEIGHT,
        "safe_area_1080p": {
            "landscape": landscape_safe,
            "portrait": portrait_safe,
        },
        "command_card": command_card,
        "control_groups": control_groups,
        "atlas": atlas,
        "assets": assets,
        "font_files": list(canonical["font_files"]),
        "provenance": {
            "file_key": extraction.get("file_key"),
            "file_name": extraction.get("file_name"),
            "fetched_at": extraction.get("fetched_at"),
            "hud_frame": {
                "id": hud["id"],
                "name": hud["name"],
                "path": hud["path"],
                "bbox": hud_box,
            },
            "export_scale": scale,
            "stats": extraction.get("stats"),
            "safe_area_source": safe_source,
            "command_card_source": command_card_source,
            "control_groups_source": control_groups_source,
            "atlas_source": "hudpack:canonical",
            "manual_insets": manual,
            "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        },
    }
    return manifest


def validate_manifest(manifest: dict, root: Path, require_fonts: bool = False) -> list[str]:
    """Mirror what hudskin/pack.rs reads; no Rust build needed."""
    errors: list[str] = []

    def need(condition: bool, message: str) -> None:
        if not condition:
            errors.append(message)

    need(isinstance(manifest.get("design_height"), (int, float)), "design_height missing")
    reference = manifest.get("reference_resolution") or {}
    for key in ("landscape", "portrait"):
        pair = reference.get(key)
        need(
            isinstance(pair, list)
            and len(pair) == 2
            and all(isinstance(value, int) and value > 0 for value in pair),
            f"reference_resolution.{key} must be [w,h]",
        )
    safe = manifest.get("safe_area_1080p") or {}
    for key in ("landscape", "portrait"):
        quad = safe.get(key)
        need(
            isinstance(quad, list)
            and len(quad) == 4
            and all(isinstance(value, (int, float)) for value in quad),
            f"safe_area_1080p.{key} must be [l,t,r,b]",
        )
    card = manifest.get("command_card") or {}
    need(isinstance(card.get("columns"), int), "command_card.columns")
    need(isinstance(card.get("rows"), int), "command_card.rows")
    need(
        isinstance(card.get("cell"), list) and len(card["cell"]) == 2,
        "command_card.cell",
    )
    need(isinstance(card.get("gap"), (int, float)), "command_card.gap")
    groups = manifest.get("control_groups") or {}
    need(isinstance(groups.get("count"), int), "control_groups.count")
    need(
        isinstance(groups.get("cell"), list) and len(groups["cell"]) == 2,
        "control_groups.cell",
    )
    atlas = (manifest.get("atlas") or {}).get("commandbuttons") or {}
    for key in ("columns", "rows"):
        need(isinstance(atlas.get(key), int), f"atlas.commandbuttons.{key}")
    need(
        isinstance(atlas.get("cell"), list) and len(atlas["cell"]) == 2,
        "atlas.commandbuttons.cell",
    )
    assets = manifest.get("assets") or []
    need(bool(assets), "assets is empty")
    for asset in assets:
        name = asset.get("name") or "<unnamed>"
        for key in ("name", "file"):
            need(isinstance(asset.get(key), str) and asset[key], f"asset {name}: {key}")
        for key in ("w", "h"):
            need(isinstance(asset.get(key), int) and asset[key] > 0, f"asset {name}: {key}")
        insets = asset.get("insets") or {}
        for key in ("l", "t", "r", "b"):
            need(
                isinstance(insets.get(key), (int, float)),
                f"asset {name}: insets.{key}",
            )
        path = root / asset.get("file", "")
        if not path.is_file():
            errors.append(f"asset {name}: missing file {asset.get('file')}")
            continue
        try:
            blob = path.read_bytes()
            width, height = png_size(blob)
        except (OSError, FigmaError) as error:
            errors.append(f"asset {name}: {error}")
            continue
        if (width, height) != (asset["w"], asset["h"]):
            errors.append(
                f"asset {name}: PNG {width}x{height} != manifest {asset['w']}x{asset['h']}"
            )
    if require_fonts:
        fonts = manifest.get("font_files") or []
        need(bool(fonts), "font_files is empty")
        for font in fonts:
            need((root / "fonts" / font).is_file(), f"missing font fonts/{font}")
    return errors


def stage_pack(out_dir: Path, manifest: dict) -> Path:
    """A loader-ready directory: manifest.json + png/ + fonts/ links."""
    pack_dir = out_dir / "pack"
    pack_dir.mkdir(parents=True, exist_ok=True)
    text = json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"
    atomic_write_text(pack_dir / "manifest.json", text)
    for name, target in (("png", out_dir / "png"), ("fonts", CANONICAL_PACK / "fonts")):
        link = pack_dir / name
        if link.is_symlink():
            link.unlink()
        elif link.exists():
            continue
        if target.exists():
            os.symlink(os.path.relpath(target, pack_dir), link)
    return pack_dir


def load_insets_overrides(path: str | None) -> dict[str, list[float]]:
    if not path:
        return {}
    data = json.loads(Path(path).expanduser().read_text(encoding="utf-8"))
    overrides: dict[str, list[float]] = {}
    for key, value in data.items():
        if isinstance(value, dict):
            value = [value.get("l", 0), value.get("t", 0), value.get("r", 0), value.get("b", 0)]
        if not (isinstance(value, list) and len(value) == 4):
            raise FigmaError(f"insets override for {key} must be [l,t,r,b]")
        overrides[str(key)] = [float(part) for part in value]
    return overrides


# --------------------------------------------------------------------------
# Modes
# --------------------------------------------------------------------------


def print_tree(extraction: dict, max_depth: int) -> None:
    hud = pick_hud_frame(extraction)
    print(
        f"file: {extraction.get('file_name')!r}  pages={extraction['stats']['pages']}  "
        f"nodes={extraction['stats']['nodes']}  frames={extraction['stats']['frames']}  "
        f"texts={extraction['stats']['texts']}"
    )
    if hud:
        box = hud["bbox"]
        print(
            f"hud frame candidate: {hud['name']!r} id={hud['id']} "
            f"{box['w']:.0f}x{box['h']:.0f} path={hud['path']}"
        )
    print(f"{'depth':>5}  {'type':<14} {'size':>13}  id / name")
    for frame in extraction["frames"]:
        if frame["depth"] > max_depth or not frame.get("bbox"):
            continue
        box = frame["bbox"]
        indent = "  " * (frame["depth"] - 1)
        print(
            f"{frame['depth']:>5}  {frame['type']:<14} "
            f"{box['w']:>6.0f}x{box['h']:<6.0f}  {indent}{frame['name']}  [{frame['id']}]"
        )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Figma -> HUD pack manifest pipeline (stdlib only).",
    )
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--file", help="Figma file key")
    source.add_argument("--file-url", help="Figma file URL (key and node-id parsed)")
    parser.add_argument("--me", action="store_true", help="check the token only")
    parser.add_argument("--list", action="store_true", help="print the frame tree and exit")
    parser.add_argument("--max-depth", type=int, default=4, help="tree depth for --list")
    parser.add_argument("--depth", type=int, default=6, help="API depth limit, 0 = full file")
    parser.add_argument("--nodes", default="", help="comma-separated node ids to export")
    parser.add_argument(
        "--scale",
        type=float,
        default=2.0,
        help="PNG export scale (1 or 2; default 2)",
    )
    parser.add_argument("--out", default=str(DEFAULT_OUT), help="output directory")
    parser.add_argument("--env-file", default="~/.config/h2/figma.env", help="FIGMA_TOKEN env file")
    parser.add_argument("--dry-run", action="store_true", help="no network: use a saved JSON")
    parser.add_argument("--input", help="saved /v1/files JSON for --dry-run")
    parser.add_argument("--exports", help="saved node-id -> PNG map for --dry-run")
    parser.add_argument("--stage-pack", action="store_true", help="write a loader-ready pack dir")
    parser.add_argument("--insets-overrides", help="JSON {node_id: [l,t,r,b]}")
    parser.add_argument(
        "--assume-zero-radius",
        action="store_true",
        help="treat a missing cornerRadius as 0 (labels the insets 'derived:assumed-square')",
    )
    args = parser.parse_args(argv)

    out_dir = Path(args.out).expanduser()
    out_dir.mkdir(parents=True, exist_ok=True)

    if args.me:
        token = load_token(args.env_file)
        me = http_json("/v1/me", token)
        print(
            f"token ok: user={me.get('handle')!r} id={me.get('id')} "
            f"email_present={bool(me.get('email'))}"
        )
        return 0

    node_hint: list[str] = []
    key = args.file
    if args.file_url:
        key, node_hint = parse_file_url(args.file_url)
    if not key:
        parser.error("--file or --file-url is required (or --me)")

    token = None
    fetched_at = None
    raw_path = out_dir / f"{key}.json"
    if args.dry_run:
        input_path = Path(args.input).expanduser() if args.input else raw_path
        if not input_path.is_file():
            raise FigmaError(f"--dry-run needs a saved file JSON: {input_path}")
        raw = json.loads(input_path.read_text(encoding="utf-8"))
        fetched_at = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    else:
        token = load_token(args.env_file)
        print(f"fetching /v1/files/{key} (depth={args.depth or 'full'})", file=sys.stderr)
        raw = fetch_file(key, token, args.depth)
        fetched_at = time.strftime("%Y-%m-%dT%H:%M:%S%z")
        atomic_write_text(raw_path, json.dumps(raw, ensure_ascii=False))
        box = raw.get("document") or {}
        print(
            f"saved {raw_path} ({raw_path.stat().st_size} bytes) "
            f"name={raw.get('name')!r} document={box.get('name')!r}"
        )

    extraction = extract(raw, key, fetched_at)
    extraction_path = out_dir / f"{key}.nodes.json"
    atomic_write_text(extraction_path, json.dumps(extraction, indent=1, ensure_ascii=False))
    stats = extraction["stats"]
    print(
        f"extracted {stats['nodes']} nodes: {stats['frames']} frames "
        f"({stats['components']} components, {stats['groups']} groups), "
        f"{stats['texts']} texts -> {extraction_path}"
    )

    if args.list:
        print_tree(extraction, args.max_depth)
        return 0

    ids: list[str] = []
    for chunk in (args.nodes or "").split(","):
        chunk = chunk.strip()
        if chunk:
            ids.append(chunk.replace("-", ":"))
    for hint in node_hint:
        if hint not in ids:
            ids.append(hint)

    exports: dict[str, dict] = {}
    if ids:
        if args.dry_run:
            exports_path = (
                Path(args.exports).expanduser()
                if args.exports
                else out_dir / f"{key}.images.json"
            )
            if not exports_path.is_file():
                raise FigmaError(
                    f"--dry-run --nodes needs a saved exports map: {exports_path}"
                )
            exports = json.loads(exports_path.read_text(encoding="utf-8"))
            for node_id, entry in exports.items():
                candidate = out_dir / entry["file"]
                if not candidate.is_file():
                    raise FigmaError(f"exports entry {node_id} points at missing {candidate}")
        else:
            assert token is not None
            nodes_by_id = {node["id"]: node for node in extraction["all_nodes"]}
            exports = export_images(key, ids, args.scale, token, out_dir, nodes_by_id)
            atomic_write_text(
                out_dir / f"{key}.images.json",
                json.dumps(exports, indent=1, ensure_ascii=False),
            )
            total = sum(entry["bytes"] for entry in exports.values())
            print(f"exported {len(exports)} PNGs ({total} bytes) -> {out_dir / 'png'}")
    elif not args.dry_run:
        print("no --nodes: nothing exported, manifest not written", file=sys.stderr)
        return 0

    if not exports:
        print("no exports available: manifest not written", file=sys.stderr)
        return 1

    canonical = load_canonical()
    overrides = load_insets_overrides(args.insets_overrides)
    if args.assume_zero_radius:
        for node in extraction["all_nodes"]:
            if (
                node.get("type") in FRAME_TYPES
                and node.get("cornerRadius") is None
                and not node.get("rectangleCornerRadii")
            ):
                node["cornerRadius"] = 0
                node["_assumed_square"] = True
    manifest = build_manifest(extraction, exports, canonical, args.scale, out_dir, overrides)
    if args.assume_zero_radius:
        for asset in manifest["assets"]:
            if asset.get("insets_source") == "derived:square":
                asset["insets_source"] = "derived:assumed-square"
    manifest_path = out_dir / "manifest.generated.json"
    atomic_write_text(
        manifest_path, json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"
    )
    print(
        f"manifest: {manifest_path} assets={len(manifest['assets'])} "
        f"manual_insets={len(manifest['provenance']['manual_insets'])} "
        f"frame={manifest['provenance']['hud_frame']['name']!r} "
        f"{manifest['reference_resolution']['landscape']}"
    )
    if manifest["provenance"]["manual_insets"]:
        for item in manifest["provenance"]["manual_insets"]:
            print(
                f"  manual insets: {item['name']} [{item['id']}] {item['reason']}",
                file=sys.stderr,
            )

    errors = validate_manifest(manifest, out_dir)
    if errors:
        for error in errors:
            print(f"manifest error: {error}", file=sys.stderr)
        return 1
    print("manifest validation: OK (pack.rs requirements mirrored)")

    if args.stage_pack:
        pack_dir = stage_pack(out_dir, manifest)
        pack_errors = validate_manifest(
            json.loads((pack_dir / "manifest.json").read_text(encoding="utf-8")),
            pack_dir,
            require_fonts=True,
        )
        if pack_errors:
            for error in pack_errors:
                print(f"staged pack error: {error}", file=sys.stderr)
            return 1
        print(f"staged loader-ready pack: {pack_dir} (manifest.json + png/ + fonts/)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except FigmaError as error:
        print(f"figma_pull: {error}", file=sys.stderr)
        raise SystemExit(1)
