"""Shared helpers: root shell, Android screen facts, PNG I/O, tiny numpy toolkit.

The QA toolkit must run on Termux with only python3 + Pillow + numpy, so every
image algorithm here is implemented locally (no scipy/opencv dependency).
"""

from __future__ import annotations

import json
import math
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
# The repo is also reachable through the legacy `wc3rust` symlink; callers may
# pass --out paths below either spelling. Keep a resolved alias so `rel()` can
# express both as relative to the same root instead of raising ValueError.
ROOT_RESOLVED = ROOT.resolve()
ANALYSIS = ROOT / "analysis" / "agent-qa"
PKG = "org.wc3rust.p0"
ACTIVITY = "org.libsdl.app.SDLActivity"
COMPONENT = f"{PKG}/{ACTIVITY}"


class QaError(RuntimeError):
    pass


def su(cmd: str, timeout: float = 60.0) -> subprocess.CompletedProcess:
    """Run one command through KernelSU root (`su -c`).

    Output is captured as bytes and decoded with replacement: `logcat` can
    contain non-UTF-8 bytes from other Android processes and must never crash a
    QA run.
    """
    proc = subprocess.run(
        ["su", "-c", cmd],
        capture_output=True,
        timeout=timeout,
        cwd=str(ROOT),
    )
    return subprocess.CompletedProcess(
        proc.args,
        proc.returncode,
        proc.stdout.decode("utf-8", "replace"),
        proc.stderr.decode("utf-8", "replace"),
    )


def su_ok(cmd: str, timeout: float = 60.0) -> str:
    proc = su(cmd, timeout=timeout)
    if proc.returncode != 0:
        raise QaError(f"su failed rc={proc.returncode}: {cmd}\n{proc.stderr.strip()[:800]}")
    return proc.stdout


def root_identity() -> str:
    out = su_ok("id")
    if "uid=0(root)" not in out:
        raise QaError(f"root unavailable: {out.strip()}")
    return out.strip()


def png_bytes(data: bytes) -> bool:
    return len(data) > 8 and data[:8] == b"\x89PNG\r\n\x1a\n"


def save_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=False) + "\n")


def rel(path: Path) -> str:
    resolved = path.resolve()
    for base in (ROOT_RESOLVED, ROOT):
        try:
            return str(resolved.relative_to(base.resolve()))
        except ValueError:
            pass
    return str(path)


# --------------------------------------------------------------------------- image


def load_rgb(path: str | Path):
    from PIL import Image

    return Image.open(path).convert("RGB")


def load_rgba(path: str | Path):
    from PIL import Image

    return Image.open(path).convert("RGBA")


def gray_array(rgb) -> "object":
    import numpy as np

    return np.asarray(rgb, dtype=np.float32).mean(axis=2)


def box_blur(gray, radius: int):
    """Uniform box blur using integral images (O(n), no scipy)."""
    import numpy as np

    radius = max(1, int(radius))
    a = np.asarray(gray, dtype=np.float32)
    pad = np.pad(a, radius, mode="edge")
    integral = pad.cumsum(axis=0).cumsum(axis=1)
    integral = np.pad(integral, ((1, 0), (1, 0)))
    k = radius * 2 + 1
    h, w = a.shape
    y0 = np.arange(h)
    x0 = np.arange(w)
    y1 = y0 + k
    x1 = x0 + k
    out = (
        integral[np.ix_(y1, x1)]
        - integral[np.ix_(y0, x1)]
        - integral[np.ix_(y1, x0)]
        + integral[np.ix_(y0, x0)]
    ) / float(k * k)
    return out


def runs_of_row(row) -> list[tuple[int, int]]:
    import numpy as np

    idx = np.flatnonzero(row)
    if idx.size == 0:
        return []
    breaks = np.flatnonzero(np.diff(idx) > 1)
    starts = np.concatenate(([idx[0]], idx[breaks + 1]))
    ends = np.concatenate((idx[breaks], [idx[-1]]))
    return list(zip(starts.tolist(), ends.tolist()))


def components(mask, min_size: int = 30) -> list[list[int]]:
    """4-neighbour connected components via row runs + union-find.

    Returns [x0, y0, x1, y1, pixel_count] with inclusive bounds.
    """
    import numpy as np

    mask = np.asarray(mask, dtype=bool)
    h, w = mask.shape
    parent: list[int] = []

    def find(a: int) -> int:
        while parent[a] != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a

    def union(a: int, b: int) -> None:
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[rb] = ra

    row_runs: list[list[tuple[int, int, int]]] = []
    prev: list[tuple[int, int, int]] = []
    for y in range(h):
        cur: list[tuple[int, int, int]] = []
        for (x0, x1) in runs_of_row(mask[y]):
            node = len(parent)
            parent.append(node)
            cur.append((x0, x1, node))
        for (x0, x1, node) in cur:
            for (px0, px1, pnode) in prev:
                if px0 <= x1 + 1 and x0 <= px1 + 1:
                    union(pnode, node)
        prev = cur
        row_runs.append(cur)

    boxes: dict[int, list[int]] = {}
    for y, cur in enumerate(row_runs):
        for (x0, x1, node) in cur:
            root = find(node)
            box = boxes.get(root)
            if box is None:
                boxes[root] = [x0, y, x1, y, x1 - x0 + 1]
            else:
                if x0 < box[0]:
                    box[0] = x0
                if x1 > box[2]:
                    box[2] = x1
                if y < box[1]:
                    box[1] = y
                if y > box[3]:
                    box[3] = y
                box[4] += x1 - x0 + 1
    return [b for b in boxes.values() if b[4] >= min_size]


def luminance(rgb: tuple[float, float, float]) -> float:
    def lin(c: float) -> float:
        c = c / 255.0
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4

    return 0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2])


def contrast_ratio(a: tuple[float, float, float], b: tuple[float, float, float]) -> float:
    la, lb = luminance(a), luminance(b)
    hi, lo = max(la, lb), min(la, lb)
    return (hi + 0.05) / (lo + 0.05)


def percentile(values, q: float) -> float:
    import numpy as np

    if not len(values):
        return 0.0
    return float(np.percentile(np.asarray(values, dtype=np.float32), q))


def now_iso() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%S%z")


def fail(msg: str) -> None:
    print(f"QA_ERROR: {msg}", file=sys.stderr)
    raise SystemExit(2)


def human_px_dp(px: float, density: float) -> float:
    return px / (density / 160.0) if density else px / 3.0


def rect_intersection(a: tuple[float, float, float, float], b: tuple[float, float, float, float]) -> float:
    ax0, ay0, ax1, ay1 = a
    bx0, by0, bx1, by1 = b
    dx = max(0.0, min(ax1, bx1) - max(ax0, bx0))
    dy = max(0.0, min(ay1, by1) - max(ay0, by0))
    return dx * dy


def rect_union_area(rects: list[tuple[float, float, float, float]]) -> float:
    return sum(max(0.0, r[2] - r[0]) * max(0.0, r[3] - r[1]) for r in rects)
