#!/usr/bin/env python3
"""Unit tests for the re-pointed HUD pack gate (tools/hud_pack_check.py).

The checker is the external half of the layout acceptance, so its own rules are
tested against synthetic matrices: a valid row must pass, and each rule must
fail when the geometry violates it. Runs without the game data or a device:

    python3 -m unittest tests/gates/test_hud_pack_check.py -v
"""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("hud_pack_check", ROOT / "tools/hud_pack_check.py")
CHECK = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = CHECK
SPEC.loader.exec_module(CHECK)

W, H, DENSITY = 2414, 1080, 3.0
SAFE = [144.0, 240.0, 144.0, 144.0]


def slot_rects() -> list[list[float]]:
    """4 columns x 3 rows of 144 px (48 dp) command slots."""
    slots = []
    for row in range(3):
        for column in range(4):
            slots.append([1500.0 + column * 150.0, 300.0 + row * 150.0, 144.0, 144.0])
    return slots


def row(**overrides) -> dict:
    base = {
        "label": "device-2414x1080",
        "framebuffer_px": [W, H],
        "density_scale": DENSITY,
        "dp_viewport": [W / DENSITY, H / DENSITY],
        "console_dp": 128.0,
        "console_px": 384.0,
        "topbar_dp": 30.0,
        "hud_pct": 43.888889,
        "viewport_pct": 56.111111,
        "min_target_dp": 48.0,
        "safe_px": list(SAFE),
        "states": [
            {
                "slug": "none",
                "interactive": [
                    {"id": "minimap", "rect": [200.0, 650.0, 240.0, 240.0]},
                    {"id": "commands_button", "rect": [2100.0, 750.0, 144.0, 144.0]},
                ],
                "card_slots": slot_rects(),
                "group_slots": [[600.0, 760.0, 144.0, 144.0]],
            }
        ],
        "gate": "ok",
    }
    base.update(overrides)
    return base


def textured_png(path: Path, size=(W, H), magenta: bool = False, flat_band: bool = False) -> Path:
    rng = np.random.default_rng(7)
    pixels = rng.integers(0, 256, size=(size[1], size[0], 3), dtype=np.uint8)
    # Keep blue <= 85 so no random pixel can satisfy the magenta mask
    # (r > 190 and b > 190). The palette stays wide enough for the
    # "real art, not flat fill" check.
    pixels[:, :, 2] = pixels[:, :, 0] // 3
    if flat_band:
        pixels[-384:, :, :] = 32
    if magenta:
        pixels[0:40, 0:40, :] = [255, 0, 255]
    Image.fromarray(pixels, "RGB").save(path)
    return path


class RowArithmetic(unittest.TestCase):
    def test_valid_row_passes(self) -> None:
        failures: list[str] = []
        CHECK.row_arithmetic(row(), failures)
        self.assertEqual(failures, [])

    def test_dp_times_density_mismatch_fails(self) -> None:
        failures: list[str] = []
        CHECK.row_arithmetic(row(dp_viewport=[800.0, 360.0]), failures)
        self.assertTrue(any("framebuffer" in failure for failure in failures), failures)

    def test_console_px_mismatch_fails(self) -> None:
        failures: list[str] = []
        CHECK.row_arithmetic(row(console_px=400.0), failures)
        self.assertTrue(any("console" in failure for failure in failures), failures)

    def test_internal_gate_fail_fails(self) -> None:
        failures: list[str] = []
        CHECK.row_arithmetic(row(gate="FAIL"), failures)
        self.assertTrue(any("internal gate" in failure for failure in failures), failures)


class RowGeometry(unittest.TestCase):
    def test_valid_row_passes(self) -> None:
        failures: list[str] = []
        states, slots = CHECK.check_row_geometry(row(), 48.0, failures)
        self.assertEqual(failures, [])
        self.assertEqual((states, slots), (1, 12))

    def test_below_touch_floor_fails(self) -> None:
        bad = row()
        bad["states"][0]["interactive"].append({"id": "tiny", "rect": [300.0, 300.0, 143.0, 144.0]})
        failures: list[str] = []
        CHECK.check_row_geometry(bad, 48.0, failures)
        self.assertTrue(any("tiny" in failure and "<" in failure for failure in failures), failures)

    def test_wrong_card_count_fails(self) -> None:
        bad = row()
        bad["states"][0]["card_slots"] = slot_rects()[:11]
        failures: list[str] = []
        CHECK.check_row_geometry(bad, 48.0, failures)
        self.assertTrue(any("11 card slots" in failure for failure in failures), failures)

    def test_wrong_card_grid_fails(self) -> None:
        bad = row()
        # 6x2 = 12 slots: the count is right, the grid is not.
        bad["states"][0]["card_slots"] = [
            [1500.0 + column * 150.0, 300.0 + row_index * 150.0, 144.0, 144.0]
            for row_index in range(2)
            for column in range(6)
        ]
        failures: list[str] = []
        CHECK.check_row_geometry(bad, 48.0, failures)
        self.assertTrue(any("card grid is 6x2" in failure for failure in failures), failures)

    def test_safe_area_violation_fails(self) -> None:
        bad = row()
        bad["states"][0]["interactive"].append({"id": "bleed", "rect": [10.0, 300.0, 144.0, 144.0]})
        failures: list[str] = []
        CHECK.check_row_geometry(bad, 48.0, failures)
        self.assertTrue(any("bleed" in failure and "safe frame" in failure for failure in failures), failures)

    def test_min_target_lie_fails(self) -> None:
        bad = row(min_target_dp=40.0)
        failures: list[str] = []
        CHECK.check_row_geometry(bad, 48.0, failures)
        self.assertTrue(any("min_target_dp" in failure for failure in failures), failures)

    def test_retired_schema_fails_loudly(self) -> None:
        old = row()
        del old["states"]
        del old["safe_px"]
        failures: list[str] = []
        CHECK.check_row_geometry(old, 48.0, failures)
        self.assertTrue(any("predates the dp matrix schema" in failure for failure in failures), failures)


class Pixels(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def test_clean_frame_passes(self) -> None:
        png = textured_png(self.dir / "clean.png", size=(480, 216))
        failures: list[str] = []
        CHECK.check_pixels(png, row(framebuffer_px=[480, 216], console_px=77.0), None, failures)
        self.assertEqual(failures, [])

    def test_magenta_placeholder_fails(self) -> None:
        png = textured_png(self.dir / "magenta.png", size=(480, 216), magenta=True)
        failures: list[str] = []
        CHECK.check_pixels(png, row(framebuffer_px=[480, 216], console_px=77.0), None, failures)
        self.assertTrue(any("magenta" in failure for failure in failures), failures)

    def test_flat_console_band_fails(self) -> None:
        png = textured_png(self.dir / "flat.png", size=(480, 216), flat_band=True)
        failures: list[str] = []
        CHECK.check_pixels(png, row(framebuffer_px=[480, 216], console_px=77.0), None, failures)
        self.assertTrue(any("flat fill" in failure for failure in failures), failures)

    def test_wrong_size_fails(self) -> None:
        png = textured_png(self.dir / "small.png", size=(240, 108))
        failures: list[str] = []
        CHECK.check_pixels(png, row(framebuffer_px=[480, 216], console_px=77.0), None, failures)
        self.assertTrue(any("size" in failure for failure in failures), failures)


if __name__ == "__main__":
    unittest.main()
