#!/usr/bin/env python3
"""Software-rasterized preview of a glTF model (no GPU / no browser required).

The Termux headless Chromium GPU process crashes (exit_code=11) on this device,
so the three.js viewer cannot be screenshotted here. This tool renders the same
glTF with a z-buffered numpy rasterizer and samples the decoded textures, which
is enough to verify geometry, orientation and the model base (pivot check).

Usage: gltf_preview.py <model.gltf> <out.png> [--size 960x640] [--yaw 35] [--pitch 22]
"""

import argparse
import math

import numpy as np
import trimesh
from PIL import Image


def look_at(eye, target, up=(0.0, 1.0, 0.0)):
    eye = np.asarray(eye, dtype=np.float64)
    target = np.asarray(target, dtype=np.float64)
    up = np.asarray(up, dtype=np.float64)
    f = target - eye
    f /= np.linalg.norm(f)
    s = np.cross(f, up)
    s /= np.linalg.norm(s)
    u = np.cross(s, f)
    view = np.eye(4)
    view[0, :3] = s
    view[1, :3] = u
    view[2, :3] = -f
    view[0, 3] = -s @ eye
    view[1, 3] = -u @ eye
    view[2, 3] = f @ eye
    return view


def rasterize(scene, out_path, width, height, yaw_deg, pitch_deg):
    geoms = list(scene.geometry.values()) if hasattr(scene, "geometry") else [scene]
    all_vertices = np.vstack([np.asarray(g.vertices) for g in geoms])
    lo = all_vertices.min(axis=0)
    hi = all_vertices.max(axis=0)
    centre = (lo + hi) * 0.5
    radius = float(np.max(hi - lo))

    yaw = math.radians(yaw_deg)
    pitch = math.radians(pitch_deg)
    eye = centre + radius * np.array(
        [math.sin(yaw) * math.cos(pitch), math.sin(pitch), math.cos(yaw) * math.cos(pitch)]
    )
    view = look_at(eye, centre)
    fov = math.radians(38.0)
    focal = (height * 0.5) / math.tan(fov * 0.5)
    near, far = radius * 0.02, radius * 8.0

    image = np.zeros((height, width, 3), dtype=np.float32)
    image[:] = np.array([0.105, 0.118, 0.141], dtype=np.float32)  # #1b1e24
    depth = np.full((height, width), np.inf, dtype=np.float32)
    light = np.array([0.45, 0.82, 0.35])
    light /= np.linalg.norm(light)

    for geom in geoms:
        vertices = np.asarray(geom.vertices, dtype=np.float64)
        faces = np.asarray(geom.faces, dtype=np.int64)
        hom = np.hstack([vertices, np.ones((len(vertices), 1))])
        eye_space = (view @ hom.T).T[:, :3]
        uvs = getattr(getattr(geom, "visual", None), "uv", None)
        material = getattr(getattr(geom, "visual", None), "material", None)
        texture = None
        if material is not None and getattr(material, "image", None) is not None:
            texture = np.asarray(material.image.convert("RGB"), dtype=np.float32) / 255.0
        for face in faces:
            p = eye_space[face]
            z = -p[:, 2]
            if np.any(z <= near) or np.any(z >= far):
                continue
            sx = width * 0.5 + focal * p[:, 0] / z
            sy = height * 0.5 - focal * p[:, 1] / z
            min_x = max(int(np.floor(sx.min())), 0)
            max_x = min(int(np.ceil(sx.max())), width - 1)
            min_y = max(int(np.floor(sy.min())), 0)
            max_y = min(int(np.ceil(sy.max())), height - 1)
            if min_x > max_x or min_y > max_y:
                continue
            x0, y0 = sx[0], sy[0]
            x1, y1 = sx[1], sy[1]
            x2, y2 = sx[2], sy[2]
            denom = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2)
            if abs(denom) < 1e-9:
                continue
            normal = np.cross(vertices[face[1]] - vertices[face[0]], vertices[face[2]] - vertices[face[0]])
            norm = np.linalg.norm(normal)
            if norm < 1e-12:
                continue
            normal /= norm
            shade = 0.35 + 0.65 * abs(float(normal @ light))
            xs = np.arange(min_x, max_x + 1) + 0.5
            ys = np.arange(min_y, max_y + 1) + 0.5
            gx, gy = np.meshgrid(xs, ys)
            w0 = ((y1 - y2) * (gx - x2) + (x2 - x1) * (gy - y2)) / denom
            w1 = ((y2 - y0) * (gx - x2) + (x0 - x2) * (gy - y2)) / denom
            w2 = 1.0 - w0 - w1
            inside = (w0 >= -1e-6) & (w1 >= -1e-6) & (w2 >= -1e-6)
            if not inside.any():
                continue
            zz = w0 * z[0] + w1 * z[1] + w2 * z[2]
            yy, xx = np.nonzero(inside)
            gyi = yy + min_y
            gxi = xx + min_x
            closer = zz[yy, xx] < depth[gyi, gxi]
            if not closer.any():
                continue
            gyi, gxi = gyi[closer], gxi[closer]
            depth[gyi, gxi] = zz[yy, xx][closer]
            if texture is not None and uvs is not None and len(uvs) >= len(vertices):
                u = w0[yy, xx][closer] * uvs[face[0]][0] + w1[yy, xx][closer] * uvs[face[1]][0] + w2[yy, xx][closer] * uvs[face[2]][0]
                v = w0[yy, xx][closer] * uvs[face[0]][1] + w1[yy, xx][closer] * uvs[face[1]][1] + w2[yy, xx][closer] * uvs[face[2]][1]
                tu = np.clip((u % 1.0) * (texture.shape[1] - 1), 0, texture.shape[1] - 1).astype(int)
                tv = np.clip((v % 1.0) * (texture.shape[0] - 1), 0, texture.shape[0] - 1).astype(int)
                colour = texture[tv, tu] * shade
            else:
                colour = np.array([0.72, 0.72, 0.75]) * shade
            image[gyi, gxi] = colour

    # Ground line at world y = 0 (pivot check) and axes hint.
    origin_eye = (view @ np.array([centre[0], 0.0, centre[2], 1.0]))[0:3]
    if -origin_eye[2] > near:
        scale = focal / -origin_eye[2]
        sx = int(width * 0.5 + origin_eye[0] * scale)
        if 0 <= sx < width:
            image[:, max(sx - 1, 0):sx + 1] = np.array([0.30, 0.45, 0.62])

    out = np.clip(image, 0.0, 1.0)
    Image.fromarray((out * 255).astype(np.uint8)).save(out_path)
    print(
        f"PREVIEW out={out_path} size={width}x{height} geoms={len(geoms)} verts={len(all_vertices)} "
        f"bbox_min={np.round(lo,2).tolist()} bbox_max={np.round(hi,2).tolist()} yaw={yaw_deg} pitch={pitch_deg}"
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("model")
    parser.add_argument("out")
    parser.add_argument("--size", default="960x640")
    parser.add_argument("--yaw", type=float, default=35.0)
    parser.add_argument("--pitch", type=float, default=22.0)
    args = parser.parse_args()
    width, height = (int(v) for v in args.size.lower().split("x"))
    scene = trimesh.load(args.model)
    rasterize(scene, args.out, width, height, args.yaw, args.pitch)


if __name__ == "__main__":
    main()
