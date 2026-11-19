#!/usr/bin/env python3
"""Pixel-level alpha and chroma-key diagnostics for HUD Artisan PNG drops.

This tool reports measurable image properties; it does not claim semantic matte
quality. It never modifies source images.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import sys
from pathlib import Path
from typing import Any

try:
    from PIL import Image, ImageChops
except ImportError as exc:  # pragma: no cover - exercised by CLI users
    raise SystemExit("Pillow is required; install tools/hud_artisan/requirements.txt") from exc

MAX_PIXELS = 64_000_000
MAX_FILE_BYTES = 256 * 1024 * 1024
DEFAULT_KEY = (255, 0, 255)


def inspect_png(path: Path, root: Path, key=DEFAULT_KEY, key_tolerance: int = 48) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        raise ValueError("input must be a regular non-symlink file")
    size = path.stat().st_size
    if size <= 0 or size > MAX_FILE_BYTES:
        raise ValueError(f"file size outside allowed range: {size}")
    with Image.open(path) as probe:
        if probe.format != "PNG":
            raise ValueError("file is not PNG")
        width, height = probe.size
        if width < 1 or height < 1 or width * height > MAX_PIXELS:
            raise ValueError(f"invalid or excessive dimensions: {width}x{height}")
        probe.verify()
    with Image.open(path) as source:
        rgba = source.convert("RGBA")
        alpha = rgba.getchannel("A")
        histogram = alpha.histogram()
        total = width * height
        transparent = histogram[0]
        opaque = histogram[255]
        semi = total - transparent - opaque
        bbox = alpha.getbbox()
        # Count non-transparent pixels close to the configured chroma key.
        # This is a warning metric, not a semantic classifier.
        rgb = rgba.convert("RGB")
        red, green, blue = rgb.split()
        key_channels = []
        for channel, value in zip((red, green, blue), key):
            reference = Image.new("L", (width, height), value)
            key_channels.append(ImageChops.difference(channel, reference))
        distance = ImageChops.add(ImageChops.add(key_channels[0], key_channels[1]), key_channels[2])
        near_mask = distance.point(lambda value: 255 if value <= key_tolerance * 3 else 0)
        visible_mask = alpha.point(lambda value: 255 if value > 0 else 0)
        near_mask = ImageChops.multiply(near_mask, visible_mask)
        near_key = near_mask.histogram()[255]
        alpha_pixels = alpha.load()
        border_points = []
        if width and height:
            border_points.extend((x, 0) for x in range(width))
            if height > 1:
                border_points.extend((x, height - 1) for x in range(width))
            border_points.extend((0, y) for y in range(1, max(1, height - 1)))
            if width > 1:
                border_points.extend((width - 1, y) for y in range(1, max(1, height - 1)))
        border_total = len(border_points)
        transparent_border = sum(alpha_pixels[x, y] == 0 for x, y in border_points)
        return {
            "file": path.relative_to(root).as_posix(),
            "sha256": _sha256(path),
            "width": width,
            "height": height,
            "bytes": size,
            "alpha": {
                "transparent_pixels": transparent,
                "semi_transparent_pixels": semi,
                "opaque_pixels": opaque,
                "transparent_ratio": round(transparent / total, 6),
                "semi_transparent_ratio": round(semi / total, 6),
                "opaque_ratio": round(opaque / total, 6),
                "bbox": list(bbox) if bbox else None,
                "transparent_border_ratio": round(transparent_border / border_total, 6) if border_total else 0.0,
            },
            "chroma_key": {
                "key_rgb": list(key),
                "near_key_opaque_pixels": near_key,
                "near_key_opaque_ratio": round(near_key / max(1, total - transparent), 6),
                "tolerance": key_tolerance,
            },
        }


def _sha256(path: Path) -> str:
    import hashlib
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def classify(item: dict[str, Any], require_alpha: bool, max_key_ratio: float) -> list[str]:
    findings: list[str] = []
    alpha = item["alpha"]
    key_ratio = item["chroma_key"]["near_key_opaque_ratio"]
    if alpha["bbox"] is None:
        findings.append("fully_transparent")
    if require_alpha and alpha["transparent_pixels"] == 0:
        findings.append("alpha_required_but_opaque")
    if key_ratio > max_key_ratio:
        findings.append("possible_chroma_key_fringe")
    if alpha["transparent_pixels"] and alpha["transparent_border_ratio"] < 0.05:
        findings.append("transparent_island_or_fully_framed_subject")
    return findings


def build_report(paths: list[Path], root: Path, require_alpha: bool = False,
                 max_key_ratio: float = 0.01, key=DEFAULT_KEY,
                 key_tolerance: int = 48) -> dict[str, Any]:
    if not math.isfinite(max_key_ratio) or not 0 <= max_key_ratio <= 1:
        raise ValueError("max_key_ratio must be between 0 and 1")
    root = root.resolve()
    items = []
    errors = []
    for path in sorted(paths, key=lambda p: p.as_posix()):
        try:
            item = inspect_png(path, root, key, key_tolerance)
            item["findings"] = classify(item, require_alpha, max_key_ratio)
            items.append(item)
        except (OSError, ValueError) as exc:
            errors.append({"file": str(path), "error": str(exc)})
    flagged = sum(bool(item["findings"]) for item in items)
    return {
        "schema_version": 1,
        "tool": "hud-artisan-alpha-qa",
        "summary": {
            "scanned": len(items),
            "flagged": flagged,
            "errors": len(errors),
            "pass": flagged == 0 and not errors,
        },
        "images": items,
        "errors": errors,
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Read-only alpha/chroma-key QA for PNG assets")
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--require-alpha", action="store_true",
                        help="flag every image that has no transparent pixels")
    parser.add_argument("--max-key-ratio", type=float, default=0.01,
                        help="flag opaque pixels near the chroma key above this ratio (default: 0.01)")
    parser.add_argument("--key", default="255,0,255", help="RGB chroma key, default 255,0,255")
    parser.add_argument("--key-tolerance", type=int, default=48)
    parser.add_argument("--strict", action="store_true", help="exit 1 if any image is flagged or unreadable")
    args = parser.parse_args(argv)
    try:
        root = args.root.resolve(strict=True)
        if not root.is_dir() or args.key_tolerance < 0 or args.key_tolerance > 255:
            raise ValueError("invalid root or key tolerance")
        key_parts = tuple(int(part) for part in args.key.split(","))
        if len(key_parts) != 3 or any(part < 0 or part > 255 for part in key_parts):
            raise ValueError("--key must contain three integers in 0..255")
        paths = [path for path in root.rglob("*.png") if not path.is_symlink()]
        report = build_report(paths, root, args.require_alpha, args.max_key_ratio,
                              key_parts, args.key_tolerance)
        args.out.parent.mkdir(parents=True, exist_ok=True)
        if args.out.is_symlink() or (args.out.exists() and not args.out.is_file()):
            raise ValueError("report output must be a regular file path, not a symlink or directory")
        import tempfile
        fd, temporary_name = tempfile.mkstemp(prefix=f".{args.out.name}.", suffix=".tmp", dir=args.out.parent)
        temporary = Path(temporary_name)
        try:
            with os.fdopen(fd, "w", encoding="utf-8", newline="\\n") as stream:
                stream.write(json.dumps(report, indent=2, ensure_ascii=False) + "\\n")
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, args.out)
        finally:
            temporary.unlink(missing_ok=True)
        print(json.dumps(report["summary"], ensure_ascii=False))
        return 1 if args.strict and not report["summary"]["pass"] else 0
    except (OSError, ValueError) as exc:
        print(f"alpha-qa: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
