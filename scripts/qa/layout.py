#!/usr/bin/env python3
"""Acceptance projection of the canonical console HUD geometry.

Source of truth (read-only during the merge):

  crates/wc3-game/src/mobile_hud.rs:423-457  `MobileHudLayout` fields
  crates/wc3-game/src/mobile_hud.rs:566-752  `MobileHud::layout` (console)
  crates/wc3-game/src/mobile_hud.rs:20       `CONSOLE_HEIGHT`
  crates/wc3-game/src/mobile_hud.rs:47-53    `SafeArea::landscape_1080/portrait_1080`
  crates/wc3-game/src/style.rs:61-90         `Tokens` (64 px cells, 4 px gaps, type sizes)

This is a *measurement projection* for the QA gate, not runtime HUD code. The
acceptance runner prefers the exact `GAME_HUD` log emitted by the read-only app
patch (`patches/0001-app-hud-rects-log.patch`); this module is the fallback and
the source of expected 1:1 values.

The 1:1 acceptance target is the original console, not the rejected mobile
rings: bottom console 288/1080 px, minimap 216, portrait 128, command card 4x3
with 64 px cells / 4 px gaps, top strip 32 px, no joystick
(`analysis/tech-lead/ACCEPTANCE_1TO1.md:5-7`).
"""

from __future__ import annotations

CONSOLE_HEIGHT = 288.0       # current source, mobile_hud.rs:20 (HUD v4)
CONSOLE_TARGET = 288.0       # ACCEPTANCE_1TO1.md:5 (1:1 console)
MINIMAP_TARGET = 216.0
PORTRAIT_TARGET = 128.0
CELL_TARGET = 64.0
GAP_TARGET = 4.0
TOP_STRIP_TARGET = 32.0
REFERENCE_HEIGHT = 1080.0


def intersect(a, b) -> float:
    dx = max(0.0, min(a[2], b[2]) - max(a[0], b[0]))
    dy = max(0.0, min(a[3], b[3]) - max(a[1], b[1]))
    return dx * dy


def clamp(value: float, lo: float, hi: float) -> float:
    return max(lo, min(hi, value))


def rect(x: float, y: float, w: float, h: float) -> tuple[float, float, float, float]:
    return (x, y, x + w, y + h)


def scale_for(w: float, h: float) -> float:
    return clamp(min(w, h) / REFERENCE_HEIGHT, 0.62, 2.0)


def default_safe(w: float, h: float) -> tuple[float, float, float, float]:
    s = scale_for(w, h)
    if w >= h * 1.25:
        return (48.0 * s, 80.0 * s, 48.0 * s, 48.0 * s)
    return (24.0 * s, 96.0 * s, 24.0 * s, 64.0 * s)


def layout(w: float, h: float, safe: dict | tuple | None = None) -> dict:
    """Return the canonical console layout plus expected 1:1 targets.

    `safe` is the SDL window safe area (left, top, right, bottom) in pixels.
    """
    if isinstance(safe, dict):
        safe_t = (
            float(safe.get("left", 0.0)),
            float(safe.get("top", 0.0)),
            float(safe.get("right", 0.0)),
            float(safe.get("bottom", 0.0)),
        )
    elif safe is not None:
        safe_t = tuple(float(v) for v in safe)  # type: ignore[assignment]
    else:
        safe_t = default_safe(w, h)
    s = scale_for(w, h)
    # mobile_hud.rs:577-581: usable frame ignores the bottom inset because the
    # console is edge-to-edge under the immersive gesture bar.
    safe_frame = rect(
        safe_t[0],
        safe_t[1],
        max(1.0, w - safe_t[0] - safe_t[2]),
        max(1.0, h - safe_t[1]),
    )
    console_h = CONSOLE_HEIGHT * s
    console = rect(safe_t[0], h - console_h, max(1.0, w - safe_t[0] - safe_t[2]), console_h)
    pad = 24.0 * s
    minimap = rect(console[0] + pad, console[1] + 48.0 * s, MINIMAP_TARGET * s, MINIMAP_TARGET * s)
    portrait = rect(console[0] + 264.0 * s, console[1] + 40.0 * s, PORTRAIT_TARGET * s, PORTRAIT_TARGET * s)
    info_x = portrait[2] + 16.0 * s
    selection = rect(portrait[0], console[1] + 6.0 * s, (info_x + 240.0 * s - portrait[0]), 168.0 * s)
    cell = CELL_TARGET * s
    gap = GAP_TARGET * s
    card = rect(
        console[2] - pad - (cell * 4.0 + gap * 3.0),
        console[1] + 64.0 * s,
        cell * 4.0 + gap * 3.0,
        cell * 3.0 + gap * 2.0,
    )
    resource_w = (720.0 if w >= h * 1.25 else (card[2] - card[0]) / s) * s
    resources = rect(card[2] - resource_w, console[1] + 4.0 * s, resource_w, 32.0 * s)
    upkeep = rect(resources[0], console[1] + 38.0 * s, resources[2] - resources[0], 14.0 * s)
    pills = []
    pill_gap = 6.0 * s
    pill_w = (card[2] - card[0] - pill_gap * 2.0) / 3.0
    for i in range(3):
        pills.append(rect(resources[0] + i * (pill_w + pill_gap), resources[1], pill_w, resources[3] - resources[1]))
    slots = []
    for row in range(3):
        for col in range(4):
            slots.append(rect(card[0] + col * (cell + gap), card[1] + row * (cell + gap), cell, cell))
    top_bar = rect(safe_t[0], safe_t[1], safe_frame[2] - safe_frame[0], TOP_STRIP_TARGET * s)
    map_name = rect(top_bar[0] + 12.0 * s, top_bar[1], (top_bar[2] - top_bar[0]) * 0.55, top_bar[3] - top_bar[1])
    fps = rect(max(top_bar[2] - 360.0 * s, top_bar[0]), top_bar[1], 360.0 * s, top_bar[3] - top_bar[1])
    target_console_h = CONSOLE_TARGET * s
    target_console = rect(safe_t[0], h - target_console_h, console[2] - console[0], target_console_h)
    return {
        "scale": s,
        "safe": safe_t,
        "safe_frame": safe_frame,
        "console": console,
        "console_target": target_console,
        "console_height": console[3] - console[1],
        "console_target_height": target_console_h,
        "minimap": minimap,
        "portrait": portrait,
        "selection": selection,
        "card": card,
        "card_slots": slots,
        "cell": cell,
        "gap": gap,
        "resources": resources,
        "resource_pills": pills,
        "upkeep": upkeep,
        "top_bar": top_bar,
        "map_name": map_name,
        "fps": fps,
        "mode": rect(0.0, 0.0, 0.0, 0.0),
        "joystick": None,
        "targets": {
            "console_height": target_console_h,
            "minimap": MINIMAP_TARGET * s,
            "portrait": PORTRAIT_TARGET * s,
            "cell": CELL_TARGET * s,
            "gap": GAP_TARGET * s,
            "top_strip": TOP_STRIP_TARGET * s,
        },
        "source": {
            "layout": "crates/wc3-game/src/mobile_hud.rs:566-752",
            "console_height": "crates/wc3-game/src/mobile_hud.rs:20",
            "acceptance_target": "analysis/tech-lead/ACCEPTANCE_1TO1.md:5-7",
            "tokens": "crates/wc3-game/src/style.rs:61-90",
        },
    }


def text_bands(lay: dict, w: int, h: int) -> list[dict]:
    """Text bands for the console HUD; sizes are the canonical Tokens values.

    style.rs:81-85: value 20 px, label 13 px, micro 11 px, title 22 px at the
    1080p reference. The 1:1 console uses these sizes; the old 24 px mobile
    floor does not apply to this HUD mode.
    """
    s = lay["scale"]
    bands: list[dict] = []

    def add(name: str, r, expected_px: float):
        bands.append({"name": name, "rect": [int(v) for v in r], "expected_px": round(expected_px, 1)})

    for i, pill in enumerate(lay["resource_pills"]):
        add(f"resource_{i}", (pill[0] + 2 * s, pill[1] + 1 * s, pill[2] - 2 * s, pill[3] - 1 * s), 20.0 * s)
    add("map_name", lay["map_name"], 22.0 * s)
    add("fps", lay["fps"], 11.0 * s)
    sel = lay["selection"]
    add("selection_name", (sel[0], sel[1], sel[2], sel[1] + 24 * s), 22.0 * s)
    add("selection_class", (sel[0], sel[1] + 24 * s, sel[2], sel[1] + 40 * s), 11.0 * s)
    add("selection_hp", (sel[0] + 80 * s, sel[1] + 44 * s, sel[2], sel[1] + 62 * s), 20.0 * s)
    add("selection_stats", (sel[0] + 80 * s, sel[1] + 84 * s, sel[2], sel[1] + 100 * s), 11.0 * s)
    for i, slot in enumerate(lay["card_slots"][:8]):
        add(f"card_{i}", (slot[0] + 2 * s, slot[3] - 16 * s, slot[2] - 2 * s, slot[3] - 2 * s), 13.0 * s)
    return bands

# Compatibility alias for the QA callers (single geometry implementation).
hud_layout = layout
