#!/usr/bin/env python3
"""Render a flat geometry preview of a converted glTF (visual check helper).

This is a rasterizer for QA eyes only: it loads the glTF through trimesh, walks
its triangles, projects them with a fixed three-quarter camera, sorts by depth
and fills them with a light-based flat shade. It is deliberately not a product
renderer and not a substitute for the game's wc3-render path — output files are
labelled `geometry preview v1 (procedural raster)`.

Usage:
  python3 scripts/qa/gltf_preview.py --gltf model.gltf --out preview.png [--size 768]
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

import numpy as np
import trimesh
from PIL import Image, ImageDraw


def project(points: np.ndarray, yaw: float, pitch: float) -> tuple[np.ndarray, np.ndarray]:
    """Rotate then orthographically project to (x, y, depth)."""
    cy, sy = math.cos(yaw), math.sin(yaw)
    cp, sp = math.cos(pitch), math.sin(pitch)
    x = points[:, 0] * cy - points[:, 2] * sy
    z = points[:, 0] * sy + points[:, 2] * cy
    y = points[:, 1] * cp - z * sp
    depth = points[:, 1] * sp + z * cp
    return np.stack([x, y], axis=1), depth


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--gltf", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--size", type=int, default=768)
    parser.add_argument("--yaw", type=float, default=math.radians(35))
    parser.add_argument("--pitch", type=float, default=math.radians(25))
    args = parser.parse_args(argv)

    scene = trimesh.load(args.gltf, force="scene")
    geometries = list(scene.geometry.values())
    if not geometries:
        print(f"PREVIEW_FAIL no geometry in {args.gltf}", file=sys.stderr)
        return 1

    triangles = []
    for geometry in geometries:
        vertices = np.asarray(geometry.vertices, dtype=np.float64)
        faces = np.asarray(geometry.faces, dtype=np.int64)
        if vertices.size == 0 or faces.size == 0:
            continue
        triangles.append((vertices, faces))
    if not triangles:
        print(f"PREVIEW_FAIL no triangles in {args.gltf}", file=sys.stderr)
        return 1

    all_vertices = np.concatenate([v for v, _ in triangles], axis=0)
    projected, depth = project(all_vertices, args.yaw, args.pitch)
    min_xy = projected.min(axis=0)
    max_xy = projected.max(axis=0)
    extent = np.maximum(max_xy - min_xy, 1e-6)
    scale = (args.size - 64) / float(max(extent))
    offset = np.array([32.0, 32.0])

    canvas = Image.new("RGB", (args.size, args.size), (18, 20, 26))
    draw = ImageDraw.Draw(canvas)
    faces_world = []
    cursor = 0
    for vertices, faces in triangles:
        for face in faces:
            idx = face + cursor
            points = projected[idx]
            centroid_depth = float(depth[idx].mean())
            normal = np.cross(
                np.append(points[1] - points[0], 0.0),
                np.append(points[2] - points[0], 0.0),
            )
            length = float(np.linalg.norm(normal))
            light = 0.45
            if length > 0:
                light = 0.35 + 0.65 * abs(float(normal[2]) / length)
            faces_world.append((centroid_depth, points, light))
        cursor += len(vertices)
    faces_world.sort(key=lambda item: item[0], reverse=True)
    for _, points, light in faces_world:
        screen = [(float(p[0] - min_xy[0]) * scale + offset[0], float(p[1] - min_xy[1]) * scale + offset[1]) for p in points]
        shade = int(max(0.0, min(1.0, light)) * 200) + 40
        color = (min(255, shade), min(255, int(shade * 0.92)), min(255, int(shade * 0.82)))
        draw.polygon(screen, fill=color)
    draw.text((10, 8), f"geometry preview v1 (procedural raster) · {len(faces_world)} tris", fill=(200, 205, 215))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    canvas.save(args.out)
    print(
        json.dumps(
            {
                "preview": str(args.out),
                "geometries": len(geometries),
                "triangles": len(faces_world),
                "size": args.size,
                "label": "geometry preview v1 (procedural raster)",
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
