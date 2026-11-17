#!/usr/bin/env python3
"""Convert PNG RGBA to BLP1 (paletted, 8-bit alpha) for the HUD redraw pack.

Header layout matches crates/wc3-formats/src/blp.rs:
  magic 'BLP1' | content_type=1 (palette) | alpha_bits=8 | width | height
  | 8 unknown bytes | 16 u32 offsets | 16 u32 sizes | 256 BGRA palette | indices | alpha
"""
import struct
import sys
from PIL import Image


def png_to_blp1(src: str, dst: str) -> None:
    im = Image.open(src).convert("RGBA")
    w, h = im.size
    rgb = im.convert("RGB")
    q = rgb.quantize(colors=256, method=Image.Quantize.FASTOCTREE, dither=Image.Dither.NONE)
    palette = q.getpalette() or []
    indices = q.tobytes()
    alpha = im.getchannel("A").tobytes()
    if len(indices) != w * h or len(alpha) != w * h:
        raise ValueError(f"{src}: index/alpha size mismatch")
    pal_bgra = bytearray(1024)
    n = len(palette) // 3
    for i in range(256):
        if i < n:
            r, g, b = palette[i * 3], palette[i * 3 + 1], palette[i * 3 + 2]
        else:
            r = g = b = 0
        pal_bgra[i * 4 + 0] = b
        pal_bgra[i * 4 + 1] = g
        pal_bgra[i * 4 + 2] = r
        pal_bgra[i * 4 + 3] = 255
    data_offset = 156 + 1024
    data_size = w * h * 2
    header = bytearray(156)
    header[0:4] = b"BLP1"
    struct.pack_into("<I", header, 4, 1)       # palette
    struct.pack_into("<I", header, 8, 8)       # alpha bits
    struct.pack_into("<I", header, 12, w)
    struct.pack_into("<I", header, 16, h)
    struct.pack_into("<I", header, 28, data_offset)
    struct.pack_into("<I", header, 92, data_size)
    with open(dst, "wb") as f:
        f.write(header)
        f.write(pal_bgra)
        f.write(indices)
        f.write(alpha)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: png_to_blp1.py <in.png> <out.blp>")
    png_to_blp1(sys.argv[1], sys.argv[2])
