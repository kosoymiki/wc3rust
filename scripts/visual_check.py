#!/usr/bin/env python3
"""Automated screenshot checker for Herofolends-RS.

Rejects the exact failures the first playable builds showed: a black texture
field, a giant white model plane, a missing HUD, and an empty 3D view. Run on
every device screenshot; exits non-zero with the failed metric.
"""
import argparse
import json
import sys
from collections import Counter

from PIL import Image


def region(px, w, h, x0, y0, x1, y1):
    return (
        int(w * x0),
        int(h * y0),
        int(w * x1),
        int(h * y1),
    )


def metrics(image):
    w, h = image.size
    px = image.load()
    out = {"size": [w, h]}
    # The world, excluding the top bar, the command card and the minimap.
    x0, y0, x1, y1 = region(px, w, h, 0.10, 0.14, 0.90, 0.72)
    green = black = white = blue = total = 0
    buckets = Counter()
    for y in range(y0, y1, 2):
        for x in range(x0, x1, 2):
            r, g, b = px[x, y][:3]
            total += 1
            if g > r + 8 and g > b + 4 and g > 40:
                green += 1
            if r + g + b < 36:
                black += 1
            if r > 232 and g > 232 and b > 232:
                white += 1
            if b > r + 18 and b > g + 6 and b > 40:
                blue += 1
            buckets[(r // 24, g // 24, b // 24)] += 1
    total = max(total, 1)
    out.update(
        green=green / total,
        black=black / total,
        white=white / total,
        blue=blue / total,
        colours=len(buckets),
    )

    # Largest near-white connected region in the world (the artifact was a
    # single huge triangle, far bigger than any HUD text).
    step = 4
    gw = (x1 - x0) // step
    gh = (y1 - y0) // step
    mask = bytearray(gw * gh)
    for gy in range(gh):
        for gx in range(gw):
            r, g, b = px[x0 + gx * step, y0 + gy * step][:3]
            if r > 232 and g > 232 and b > 232:
                mask[gy * gw + gx] = 1
    seen = bytearray(gw * gh)
    largest = 0
    for start in range(gw * gh):
        if not mask[start] or seen[start]:
            continue
        stack = [start]
        seen[start] = 1
        size = 0
        while stack:
            index = stack.pop()
            size += 1
            x, y = index % gw, index // gw
            for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
                if 0 <= nx < gw and 0 <= ny < gh:
                    n = ny * gw + nx
                    if mask[n] and not seen[n]:
                        seen[n] = 1
                        stack.append(n)
        largest = max(largest, size)
    out["white_component"] = largest / max(gw * gh, 1)

    # HUD: command card and top bar must not be empty.
    cx0, cy0, cx1, cy1 = region(px, w, h, 0.72, 0.78, 0.995, 0.99)
    command = Counter()
    for y in range(cy0, cy1, 3):
        for x in range(cx0, cx1, 3):
            r, g, b = px[x, y][:3]
            command[(r // 32, g // 32, b // 32)] += 1
    out["command_colours"] = len(command)
    tx0, ty0, tx1, ty1 = region(px, w, h, 0.0, 0.0, 1.0, 0.06)
    top = Counter()
    for y in range(ty0, ty1, 2):
        for x in range(tx0, tx1, 4):
            r, g, b = px[x, y][:3]
            top[(r // 32, g // 32, b // 32)] += 1
    out["top_colours"] = len(top)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("screenshot")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--min-green", type=float, default=0.15)
    args = ap.parse_args()
    image = Image.open(args.screenshot).convert("RGB")
    m = metrics(image)
    failures = []
    if m["colours"] < 30:
        failures.append(f"scene has only {m['colours']} colour buckets")
    if m["green"] < args.min_green:
        failures.append(f"terrain green {m['green']:.3f} < {args.min_green}")
    if m["black"] > 0.22:
        failures.append(f"black holes {m['black']:.3f} > 0.22")
    if m["white"] > 0.08:
        failures.append(f"white field {m['white']:.3f} > 0.08")
    if m["white_component"] > 0.03:
        failures.append(f"giant white component {m['white_component']:.3f} > 0.03")
    if m["blue"] < 0.001:
        failures.append(f"units not visible: blue {m['blue']:.4f}")
    if m["command_colours"] < 3:
        failures.append(f"command card empty ({m['command_colours']} colours)")
    if m["top_colours"] < 3:
        failures.append(f"top bar empty ({m['top_colours']} colours)")
    if args.json:
        print(json.dumps({"metrics": m, "failures": failures}, indent=2))
    else:
        print("VISUAL " + " ".join(f"{k}={v}" for k, v in m.items() if k != "size"))
        for failure in failures:
            print("FAIL", failure)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
