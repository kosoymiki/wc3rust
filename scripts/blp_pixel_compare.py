#!/usr/bin/env python3
"""Byte-level pixel check: our BLP decoder vs an independent BLP1 reader.

Picks paletted BLP1 files from the shipped tree, decodes them twice — once with
`crates/wc3-formats` (`--example blp_hash`) and once with the standalone reader
below — and compares the SHA-256 of the RGBA buffer. A mismatch means one of the
two readers is wrong; the file's own header decides which.

BLP1 paletted layout (verified against the shipped files):
    magic "BLP1", u32 content_type(=1), u32 alpha_bits, u32 width, u32 height,
    u32 mip_offsets[16] @28, u32 mip_sizes[16] @92, 256 BGRA palette @156,
    mip0 = indices (w*h) followed by the alpha plane
    (0 bits: none, 8: w*h, 4: ceil(w*h/2), 1: ceil(w*h/8)).
    RGBA = (palette.r, palette.g, palette.b, alpha).

Usage: python3 scripts/blp_pixel_compare.py [--json] [--limit N]
Exit:  0 all compared, 1 any mismatch/error.
"""

from __future__ import annotations

import hashlib
import json
import os
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TREE = ROOT / "analysis" / "war3_full3"
# original_parity builds every tool once and exports the directory: executing the
# built binary keeps this check off the shared cargo queue. Standalone runs fall
# back to `cargo run`.
BIN_DIR = os.environ.get("WC3RUST_BIN_DIR")


def u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def alpha_plane_len(count: int, bits: int) -> int:
    return {0: 0, 8: count, 4: (count + 1) // 2, 1: (count + 7) // 8}.get(bits, 0)


def alpha_sample(plane: bytes, bits: int, index: int) -> int:
    if bits == 0:
        return 255
    if bits == 8:
        return plane[index]
    if bits == 4:
        byte = plane[index // 2]
        nibble = (byte & 0x0F) if index % 2 == 0 else (byte >> 4)
        return nibble * 17
    if bits == 1:
        return 255 if plane[index // 8] & (1 << (index % 8)) else 0
    return 255


def decode_blp1(data: bytes) -> tuple[int, int, bytes]:
    if data[:4] != b"BLP1":
        raise ValueError("not BLP1")
    content_type = u32(data, 4)
    if content_type != 1:
        raise ValueError(f"content_type={content_type} (not paletted)")
    bits = u32(data, 8)
    width = u32(data, 12)
    height = u32(data, 16)
    if width == 0 or height == 0:
        raise ValueError("zero dimensions")
    mip0 = u32(data, 28)
    palette = data[156:156 + 1024]
    if len(palette) < 1024:
        raise ValueError("palette truncated")
    count = width * height
    indices = data[mip0:mip0 + count]
    if len(indices) < count:
        raise ValueError("index plane truncated")
    plane_len = alpha_plane_len(count, bits)
    plane = data[mip0 + count:mip0 + count + plane_len]
    if len(plane) < plane_len:
        raise ValueError("alpha plane truncated")
    out = bytearray(count * 4)
    for i, palette_index in enumerate(indices):
        p = palette_index * 4
        a = alpha_sample(plane, bits, i)
        o = i * 4
        out[o] = palette[p + 2]
        out[o + 1] = palette[p + 1]
        out[o + 2] = palette[p]
        out[o + 3] = a
    return width, height, bytes(out)


def pick_files(limit: int) -> list[Path]:
    picked: list[Path] = []
    seen_bits: set[int] = set()
    for path in sorted(TREE.rglob("*.blp")):
        try:
            data = path.read_bytes()
        except OSError:
            continue
        if data[:4] != b"BLP1" or u32(data, 4) != 1:
            continue
        bits = u32(data, 8)
        if bits not in (0, 1, 4, 8):
            continue
        # Prefer variety in the alpha depth, then fill up to the limit.
        if bits not in seen_bits or len(picked) < limit:
            seen_bits.add(bits)
            picked.append(path)
        if len(picked) >= limit:
            break
    return picked


def main() -> int:
    as_json = "--json" in sys.argv[1:]
    limit = 8
    if "--limit" in sys.argv[1:]:
        limit = int(sys.argv[sys.argv.index("--limit") + 1])

    files = pick_files(limit)
    if not files:
        print("BLP_PIXEL_SUMMARY compared=0 (no paletted BLP1 in the tree)")
        return 0

    exe = Path(BIN_DIR) / "examples" / "blp_hash" if BIN_DIR else None
    if exe is not None and exe.is_file():
        command = [str(exe)] + [str(p) for p in files]
    else:
        command = (["scripts/h2-cargo", "run", "-q", "-p", "wc3-formats",
                    "--example", "blp_hash", "--offline", "--"] + [str(p) for p in files])
    proc = subprocess.run(
        command,
        cwd=str(ROOT), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=600,
    )
    ours: dict[str, tuple[int, int, str]] = {}
    for line in proc.stdout.decode("utf-8", "replace").splitlines():
        parts = line.split("|")
        if len(parts) == 4:
            ours[parts[0]] = (int(parts[1]), int(parts[2]), parts[3])

    rows = []
    failures = 0
    for path in files:
        key = str(path)
        w, h, rgba = decode_blp1(path.read_bytes())
        expected = hashlib.sha256(rgba).hexdigest()
        got = ours.get(key)
        ok = got is not None and got[0] == w and got[1] == h and got[2] == expected
        if not ok:
            failures += 1
        rows.append({
            "file": key.replace(str(ROOT) + "/", ""),
            "size": [w, h],
            "independent": expected,
            "ours": got[2] if got else None,
            "ok": ok,
        })

    if as_json:
        print(json.dumps({"ok": failures == 0, "compared": len(files), "failures": failures,
                          "rows": rows}, ensure_ascii=False, indent=2))
    else:
        for row in rows:
            if not row["ok"]:
                print(f"BLP_PIXEL_MISMATCH {row['file']} independent={row['independent'][:12]} ours={(row['ours'] or 'none')[:12]}")
        print(f"BLP_PIXEL_SUMMARY compared={len(files)} failures={failures} rc={proc.returncode}")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
