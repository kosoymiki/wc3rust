"""Offline unit tests for the art-push magenta scanner classifications.

Run: python3 -m pytest scripts/qa/tests/test_magenta_scan.py -q
No device, build or cargo invocation.
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[3]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts.qa.art_push_magenta_scan import scan

SIZE = (64, 64)


def write_png(path: Path, rgb: tuple[int, int, int]) -> Path:
    data = np.zeros((SIZE[1], SIZE[0], 3), dtype=np.uint8)
    data[:, :] = rgb
    Image.fromarray(data).save(path)
    return path


def test_clean_frame_passes(tmp_path):
    rec = scan(write_png(tmp_path / "frame.png", (20, 120, 40)), SIZE, False)
    assert rec["verdict"] == "PASS"
    assert rec["pink_pixels"] == 0


def test_pink_frame_fails(tmp_path):
    rec = scan(write_png(tmp_path / "frame.png", (240, 10, 230)), SIZE, False)
    assert rec["verdict"] == "FAIL"
    assert rec["pink_pixels"] > 0


def test_source_atlas_with_pink_icon_art_is_classified(tmp_path):
    rec = scan(write_png(tmp_path / "atlas.png", (220, 20, 230)), SIZE, False,
               source_atlas=True)
    assert rec["verdict"] == "N/A-source-atlas"
    assert rec["pink_pixels"] > 0
    assert rec["exact_magenta_pixels"] == 0


def test_source_atlas_exact_chroma_key_fails(tmp_path):
    rec = scan(write_png(tmp_path / "atlas.png", (255, 0, 255)), SIZE, False,
               source_atlas=True)
    assert rec["verdict"] == "FAIL"
    assert rec["exact_magenta_pixels"] > 0


def test_unreadable_image_is_a_hard_fail(tmp_path):
    broken = tmp_path / "broken.png"
    broken.write_bytes(b"")
    rec = scan(broken, None, False)
    assert rec["verdict"] == "FAIL"
    assert "cannot identify" in rec["error"]
