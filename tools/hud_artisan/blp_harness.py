#!/usr/bin/env python3
"""BLP1/BLP2 header inspector and external-converter round-trip harness.

No codec is bundled or assumed. Supply local encoder/decoder argv templates;
the harness validates BLP headers and measures the decoded PNG against source.
"""
from __future__ import annotations
import argparse, json, math, os, struct, subprocess, tempfile
from pathlib import Path
from typing import Any

from PIL import Image, ImageChops

MAX_DIMENSION = 16384
MAX_PIXELS = 64_000_000
HEADER_BLP2 = 148
HEADER_BLP1 = 1180


class BLPError(ValueError):
    pass


def inspect_blp(path: Path) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        raise BLPError("BLP input must be a regular non-symlink file")
    size = path.stat().st_size
    with path.open("rb") as f:
        header = f.read(HEADER_BLP1)
    if len(header) < 8:
        raise BLPError("truncated BLP header")
    magic = header[:4]
    if magic == b"BLP2":
        if len(header) < HEADER_BLP2:
            raise BLPError("truncated BLP2 header")
        kind, encoding, alpha_depth, alpha_encoding, has_mips, width, height = struct.unpack_from("<IBBBBII", header, 4)
        offsets = struct.unpack_from("<16I", header, 20)
        sizes = struct.unpack_from("<16I", header, 84)
        if kind != 1:
            raise BLPError(f"unsupported BLP2 type {kind}")
        if encoding not in (1, 2, 3):
            raise BLPError(f"unsupported BLP2 encoding {encoding}")
        info = {"format": "BLP2", "encoding": encoding, "alpha_depth": alpha_depth,
                "alpha_encoding": alpha_encoding, "has_mips": bool(has_mips)}
        header_size = HEADER_BLP2
    elif magic == b"BLP1":
        if len(header) < HEADER_BLP1:
            raise BLPError("truncated BLP1 header")
        compression, flags, width, height, picture_type, picture_subtype = struct.unpack_from("<6I", header, 4)
        offsets = struct.unpack_from("<16I", header, 28)
        sizes = struct.unpack_from("<16I", header, 92)
        if compression not in (0, 1):
            raise BLPError(f"unsupported BLP1 compression {compression}")
        info = {"format": "BLP1", "compression": compression, "flags": flags,
                "picture_type": picture_type, "picture_subtype": picture_subtype}
        header_size = HEADER_BLP1
    else:
        raise BLPError(f"unknown BLP magic {magic!r}")
    if not (1 <= width <= MAX_DIMENSION and 1 <= height <= MAX_DIMENSION and width * height <= MAX_PIXELS):
        raise BLPError(f"invalid or excessive dimensions {width}x{height}")
    if offsets[0] < header_size or sizes[0] <= 0 or offsets[0] + sizes[0] > size:
        raise BLPError("invalid first mipmap offset/size")
    if has_mips if magic == b"BLP2" else any(offsets[1:]):
        for index, (offset, length) in enumerate(zip(offsets, sizes)):
            if offset == 0 and length == 0:
                continue
            if offset < header_size or length <= 0 or offset + length > size:
                raise BLPError(f"invalid mipmap {index} offset/size")
    return {"path": str(path), "bytes": size, "width": width, "height": height,
            "mipmap_offsets": list(offsets), "mipmap_sizes": list(sizes), **info}


def command_run(template: list[str], source: Path, destination: Path, timeout: int) -> None:
    if not template or not all(isinstance(x, str) for x in template):
        raise BLPError("converter command must be a non-empty argv list")
    if not any("{input}" in x for x in template) or not any("{output}" in x for x in template):
        raise BLPError("converter argv must include {input} and {output}")
    argv = [x.replace("{input}", str(source)).replace("{output}", str(destination)) for x in template]
    result = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, timeout=timeout, check=False)
    if result.returncode:
        raise BLPError(f"converter exit {result.returncode}: {result.stderr.decode(errors='replace')[-1200:]}")
    if not destination.is_file() or destination.is_symlink():
        raise BLPError("converter did not create a regular output file")


def compare_png(source: Path, decoded: Path) -> dict[str, Any]:
    with Image.open(source) as a0, Image.open(decoded) as b0:
        a0.load(); b0.load()
        if a0.format != "PNG" or b0.format != "PNG":
            raise BLPError("round-trip source and decoded output must be PNG")
        a = a0.convert("RGBA"); b = b0.convert("RGBA")
        if a.size != b.size:
            raise BLPError(f"round-trip dimensions differ: {a.size} vs {b.size}")
        if a.width * a.height > MAX_PIXELS:
            raise BLPError("pixel budget exceeded")
        diff = ImageChops.difference(a, b)
        hist = diff.histogram()
        samples = a.width * a.height * 4
        mse = sum((i % 256) ** 2 * n for i, n in enumerate(hist)) / samples
        rmse = math.sqrt(mse)
        psnr = float("inf") if mse == 0 else 20 * math.log10(255.0 / rmse)
        aa = a.getchannel("A").point(lambda x: 255 if x >= 32 else 0)
        ba = b.getchannel("A").point(lambda x: 255 if x >= 32 else 0)
        inter = ImageChops.multiply(aa, ba).histogram()[255]
        union = ImageChops.lighter(aa, ba).histogram()[255]
        return {"width": a.width, "height": a.height, "rmse_rgba": round(rmse, 4),
                "psnr_rgba_db": None if math.isinf(psnr) else round(psnr, 4),
                "exact_match": mse == 0, "alpha_silhouette_iou": round(inter / max(1, union), 5)}


def roundtrip(source: Path, work: Path, encode: list[str], decode: list[str], timeout: int) -> dict[str, Any]:
    if source.is_symlink() or not source.is_file():
        raise BLPError("source PNG must be a regular non-symlink file")
    with Image.open(source) as im:
        im.load()
        if im.format != "PNG" or im.width * im.height > MAX_PIXELS:
            raise BLPError("source must be a valid PNG within the pixel budget")
    work.mkdir(parents=True, exist_ok=True)
    blp = work / (source.stem + ".roundtrip.blp")
    decoded = work / (source.stem + ".decoded.png")
    for path in (blp, decoded):
        if path.exists() or path.is_symlink():
            if path.is_symlink() or not path.is_file():
                raise BLPError(f"unsafe output path: {path}")
            path.unlink()
    command_run(encode, source, blp, timeout)
    blp_info = inspect_blp(blp)
    command_run(decode, blp, decoded, timeout)
    metrics = compare_png(source, decoded)
    return {"source": str(source), "blp": blp_info, "roundtrip": metrics,
            "encoder_argv": encode, "decoder_argv": decode}


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description="BLP1/BLP2 validation and codec round-trip harness")
    sub = p.add_subparsers(dest="cmd", required=True)
    q = sub.add_parser("inspect")
    q.add_argument("--input", type=Path, required=True)
    q = sub.add_parser("roundtrip")
    q.add_argument("--input", type=Path, required=True)
    q.add_argument("--work", type=Path, required=True)
    q.add_argument("--encode-arg", action="append", required=True,
                   help="one argv token; use {input} and {output} placeholders")
    q.add_argument("--decode-arg", action="append", required=True)
    q.add_argument("--timeout", type=int, default=120)
    q.add_argument("--report", type=Path, required=True)
    a = p.parse_args(argv)
    try:
        if a.cmd == "inspect":
            result = inspect_blp(a.input)
        else:
            if not 1 <= a.timeout <= 3600:
                raise BLPError("timeout must be 1..3600 seconds")
            result = roundtrip(a.input, a.work, a.encode_arg, a.decode_arg, a.timeout)
            a.report.parent.mkdir(parents=True, exist_ok=True)
            if a.report.is_symlink():
                raise BLPError("report path must not be a symlink")
            temp = a.report.with_name("." + a.report.name + ".tmp")
            temp.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
            os.replace(temp, a.report)
        print(json.dumps(result, indent=2, ensure_ascii=False, allow_nan=False))
        return 0
    except (OSError, BLPError, ValueError, subprocess.TimeoutExpired) as exc:
        p.exit(2, f"BLP_HARNESS_ERROR: {exc}\n")


if __name__ == "__main__":
    raise SystemExit(main())
