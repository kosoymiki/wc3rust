#!/usr/bin/env python3
"""u2net saliency matte for HUD Artisan renders (ONNX Runtime, Android/Termux).

ComfyUI produces RGB frames; alpha-required HUD assets need a silhouette mask.
This module turns a rendered PNG into RGBA using the u2net salient-object model
(the same model family rembg uses) through onnxruntime. Deterministic for a given
model and input. It never touches source images when used from the adapter; the
CLI writes a separate output file.

The model is not bundled: place u2net.onnx locally and point --model or U2NET_MODEL
at it (default ~/src/comfyui-termux/models/u2net.onnx).
"""
from __future__ import annotations

import argparse
import os
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image

MEAN = (0.485, 0.456, 0.406)
STD = (0.229, 0.224, 0.225)
SIZE = 320
DEFAULT_MODEL = Path.home() / "src" / "comfyui-termux" / "models" / "u2net.onnx"

_sessions: dict[str, object] = {}


def prepare_input(image: Image.Image, size: int = SIZE) -> np.ndarray:
    """RGB -> normalized [1, 3, size, size] float32, the u2net input contract."""
    if type(size) is not int or not 1 <= size <= 2048:
        raise ValueError("input size must be an integer in 1..2048")
    if image.width <= 0 or image.height <= 0:
        raise ValueError("input image dimensions must be positive")
    rgb = image.convert("RGB").resize((size, size), Image.Resampling.LANCZOS)
    arr = np.asarray(rgb, dtype=np.float32) / 255.0
    arr = (arr - np.asarray(MEAN, dtype=np.float32)) / np.asarray(STD, dtype=np.float32)
    return np.ascontiguousarray(arr.transpose(2, 0, 1)[None])


def mask_from_prediction(pred: np.ndarray, size: tuple[int, int]) -> Image.Image:
    """Min/max-normalized 8-bit mask resized to the native image size."""
    pred = np.asarray(pred, dtype=np.float32)
    if pred.ndim != 2 or pred.size == 0:
        raise ValueError(f"u2net mask must be a non-empty 2-D array, got shape {pred.shape}")
    if not np.isfinite(pred).all():
        raise ValueError("u2net mask contains NaN or infinite values")
    if (not isinstance(size, tuple) or len(size) != 2
            or any(type(value) is not int or value <= 0 or value > 16384 for value in size)):
        raise ValueError("output size must be a pair of positive integers no larger than 16384")
    low, high = float(pred.min()), float(pred.max())
    if high - low < 1e-8:
        raise ValueError("u2net returned a constant mask; cannot build a silhouette")
    out = (pred - low) / (high - low) * 255.0
    return Image.fromarray(out.astype(np.uint8), "L").resize(size, Image.Resampling.LANCZOS)


def get_session(model_path: Path | str | None = None):
    path = Path(model_path or os.environ.get("U2NET_MODEL") or DEFAULT_MODEL).expanduser()
    if path.is_symlink() or not path.is_file():
        raise FileNotFoundError(f"u2net model must be a regular non-symlink file: {path}")
    path = path.resolve(strict=True)
    key = str(path)
    if key not in _sessions:
        try:
            import onnxruntime as ort  # lazy: only alpha-required jobs need it
        except ImportError as exc:
            raise RuntimeError(
                "U2Net matte mode requires ONNX Runtime; install tools/hud_artisan/requirements-alpha-matte.txt"
            ) from exc
        _sessions[key] = ort.InferenceSession(key, providers=["CPUExecutionProvider"])
    return _sessions[key]


def matte_rgba(image: Image.Image, model_path: Path | str | None = None, session=None) -> Image.Image:
    """Return a copy of `image` as RGBA with the u2net silhouette as alpha."""
    rgba = image.convert("RGBA").copy()
    sess = session if session is not None else get_session(model_path)
    inputs = sess.get_inputs()
    if not inputs or not isinstance(getattr(inputs[0], "name", None), str) or not inputs[0].name:
        raise ValueError("u2net session exposes no valid input tensor")
    outputs = sess.run(None, {inputs[0].name: prepare_input(rgba)})
    if not outputs:
        raise ValueError("u2net session returned no output tensors")
    first = np.asarray(outputs[0])
    if first.ndim != 4 or first.shape[0] != 1 or first.shape[1] != 1 or first.shape[2] < 2 or first.shape[3] < 2:
        raise ValueError(f"unexpected u2net output shape {first.shape}")
    alpha = mask_from_prediction(first[0][0], rgba.size)
    rgba.putalpha(alpha)
    return rgba


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Build an RGBA matte with u2net (onnxruntime)")
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model", type=Path)
    args = parser.parse_args(argv)
    if args.input.is_symlink() or not args.input.is_file():
        raise SystemExit("input must be a regular non-symlink file")
    with Image.open(args.input) as image:
        image.load()
        result = matte_rgba(image, args.model)
    if args.output.is_symlink() or (args.output.exists() and not args.output.is_file()):
        raise SystemExit("output must be a regular file path, not a symlink or directory")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    output_parent = args.output.parent.resolve()
    output_path = output_parent / args.output.name
    temporary_path = None
    try:
        with tempfile.NamedTemporaryFile(dir=output_parent, suffix=".png.tmp", delete=False) as temporary:
            temporary_path = Path(temporary.name)
        result.save(temporary_path, format="PNG", optimize=True, compress_level=9)
        temporary_path.replace(output_path)
    finally:
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)
    transparent = sum(result.getchannel("A").histogram()[:9])
    ratio = transparent / max(1, result.width * result.height)
    print(f"MATTE_OK output={output_path} size={result.size[0]}x{result.size[1]} near_transparent={ratio:.3f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
