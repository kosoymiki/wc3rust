#!/usr/bin/env python3
"""One P0 device cycle: launch, HUD evidence, control groups, minimap, tooltip,
command card, build, train, box/getevent, memory — in a single app session.

Exactly one launch. Every step records what it observed (logcat line, selection
count, pixel statistics, screenshot path); a step that cannot be proven is
recorded as `observed: false` with the reason instead of being retried. The
script never installs, never rebuilds, and always returns the device to Termux.

Usage:
  python3 scripts/qa/device_p0_cycle.py --apk build/android-p0/release/wc3rust-p0.apk \
      --out analysis/agent-qa/parity/p0-device
"""

from __future__ import annotations

import os
if os.environ.get("H2_DEVICE_SLOT") != "1":
    print("DEVICE_SLOT: not granted (set H2_DEVICE_SLOT=1 explicitly)")
    raise SystemExit(3)

import argparse
import hashlib
import importlib.util
import json
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

import numpy as np
from PIL import Image

from scripts.qa import analyze
from scripts.qa.common import COMPONENT, PKG, QaError
from scripts.qa.device import Device

ACCEPT = ROOT / "analysis" / "agent-qa" / "acceptance"
if str(ACCEPT) not in sys.path:
    sys.path.insert(0, str(ACCEPT))
import console_layout  # noqa: E402

_spec = importlib.util.spec_from_file_location("acceptance_helpers", ACCEPT / "run_acceptance.py")
helpers = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(helpers)
state_samples = helpers.state_samples
parse_game_hud = helpers.parse_game_hud
world_rect = helpers.world_rect

GETEVENT_REMOTE = "/data/local/tmp/qa-p0-getevent.log"
# Pixel offsets around the screen centre. The BASE card centres the camera on
# the town hall, so these are stable positions next to the base footprint.
PLACEMENT_OFFSETS = [
    (0, -150), (150, -80), (-150, -80), (190, 90), (-190, 90),
    (0, 170), (260, -170), (-260, -170), (120, -260), (-120, -260),
]


def now_utc() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256(path: Path) -> str | None:
    if not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def last_state(logs: str) -> dict:
    rows = state_samples(logs)
    return rows[-1] if rows else {}


def selected_now(device: Device) -> int:
    return int(last_state(device.logs()).get("selected", 0) or 0)


def wait_selected(device: Device, predicate, timeout: float = 4.0) -> int:
    deadline = time.time() + timeout
    value = selected_now(device)
    while time.time() < deadline:
        value = selected_now(device)
        if predicate(value):
            return value
        time.sleep(0.25)
    return value


def blue_boxes(image: np.ndarray, lay: dict, w: int, h: int) -> list[list[int]]:
    rect = world_rect(lay, w, h)
    units = analyze.detect_units(image, rect)
    boxes = []
    for cluster in units.get("blue_clusters", []):
        bw = cluster[2] - cluster[0] + 1
        bh = cluster[3] - cluster[1] + 1
        if 4 <= min(bw, bh) and max(bw, bh) <= 140 and cluster[4] >= 8:
            boxes.append(
                [
                    rect[0] + cluster[0],
                    rect[1] + cluster[1],
                    rect[0] + cluster[2],
                    rect[1] + cluster[3],
                ]
            )
    return boxes


def crop_array(path: Path, rect: list[float] | tuple[float, ...]) -> np.ndarray | None:
    if not path.is_file():
        return None
    image = np.asarray(Image.open(path).convert("RGB"))
    x0, y0, x1, y1 = (int(round(v)) for v in rect)
    x0, y0 = max(0, x0), max(0, y0)
    x1, y1 = min(image.shape[1], x1), min(image.shape[0], y1)
    if x1 <= x0 or y1 <= y0:
        return None
    return image[y0:y1, x0:x1]


def stats(pixels: np.ndarray | None) -> dict:
    if pixels is None or pixels.size == 0:
        return {"available": False}
    return {
        "available": True,
        "mean": round(float(pixels.mean()), 3),
        "std": round(float(pixels.std()), 3),
        "dark_frac": round(float((pixels.mean(axis=2) < 40).mean()), 4),
        "bright_frac": round(float((pixels.mean(axis=2) > 170).mean()), 4),
    }


def mean_abs_diff(a: np.ndarray | None, b: np.ndarray | None) -> float | None:
    if a is None or b is None or a.shape != b.shape:
        return None
    return round(float(np.abs(a.astype(np.int16) - b.astype(np.int16)).mean()), 3)


def group_slots(lay: dict) -> list[list[float]]:
    """Group-row rects from game.rs:1166-1180 (top bar, centred 10 slots)."""
    bar = lay["top_bar"]
    s = lay["scale"]
    size = 40.0 * s
    gap = 6.0 * s
    total = size * 10.0 + gap * 9.0
    x0 = bar[0] + max(0.0, (bar[2] - bar[0] - total) * 0.5)
    y = bar[1] + 12.0 * s
    return [[x0 + i * (size + gap), y, x0 + i * (size + gap) + size, y + size] for i in range(10)]


def centre(rect: list[float]) -> tuple[float, float]:
    return ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--apk", type=Path, default=ROOT / "build" / "android-p0" / "release" / "wc3rust-p0.apk")
    parser.add_argument("--out", type=Path, default=ROOT / "analysis" / "agent-qa" / "parity" / "p0-device")
    args = parser.parse_args(argv)
    out = args.out.resolve()
    shots = out / "screens"
    shots.mkdir(parents=True, exist_ok=True)

    device = Device()
    cycle: dict = {
        "schema": "wc3rust-p0-device-cycle-v1",
        "timestamp": now_utc(),
        "apk": {"path": str(args.apk), "sha256": sha256(args.apk)},
        "steps": {},
        "status": "FAIL",
    }
    logcat_at = lambda: device.logs()  # noqa: E731
    try:
        launch = device.launch(apk=args.apk, install=False)
        display = launch["display"]
        w, h = display["w"], display["h"]
        safe = launch.get("safe_insets") or launch["insets"]
        cycle["package"] = device.package_version()
        cycle["display"] = display
        cycle["launch"] = {
            "first_frame": launch.get("first_frame"),
            "insets": launch.get("insets"),
            "safe_insets": safe,
        }
        lay = {**console_layout.layout(w, h, safe), **(parse_game_hud(device.logs()) or {})}
        slots = lay.get("card_slots") or []
        if len(slots) < 8:
            raise QaError(f"card slots={len(slots)} (need >=8)")
        cycle["layout"] = {
            "console": lay["console"],
            "console_target": lay["targets"]["console_height"],
            "minimap": lay["minimap"],
            "card": lay["card"],
            "resource_pills": lay["resource_pills"],
            "top_bar": lay["top_bar"],
            "slot_count": len(slots),
        }

        def shot(name: str) -> Path:
            path = shots / name
            device.screenshot(path, display)
            return path

        def rel(path: Path) -> str:
            return str(path.relative_to(ROOT))

        # 1. launch + HUD console/resources -----------------------------------
        start = shot("01-launch.png")
        state0 = last_state(device.logs())
        console_px = crop_array(start, lay["console"])
        pill_stats = [stats(crop_array(start, pill)) for pill in lay["resource_pills"]]
        cycle["steps"]["launch"] = {
            "screenshot": rel(start),
            "first_frame": (launch.get("first_frame") or {}).get("first_frame"),
            "ready": (launch.get("first_frame") or {}).get("ready"),
            "state": state0,
            "console_pixels": stats(console_px),
            "resource_pills": pill_stats,
            "console_on_screen": bool(lay["console"][1] >= 0 and lay["console"][3] <= h + 0.5),
            "console_height": round(lay["console"][3] - lay["console"][1], 2),
            "console_target": round(lay["targets"]["console_height"], 2),
        }

        # 2. control groups: save -> clear -> recall --------------------------
        before_army = device.logs().count("BUTTON ARMY")
        device.tap(*centre(slots[1]))
        time.sleep(0.8)
        army_selected = wait_selected(device, lambda v: v >= 1)
        group = group_slots(lay)
        gx, gy = centre(group[0])
        device.long_press(gx, gy, ms=700)
        time.sleep(0.8)
        saved_shot = shot("02-group-saved.png")
        # Reduce the selection to a single unit by tapping one detected unit,
        # so the recall below has to change the count back to the saved group.
        # Change the selection through the command card (world picks are not
        # reliable while units animate): BASE selects the town hall, so the
        # recall below must restore the six saved army units.
        device.tap(*centre(slots[2]))
        time.sleep(0.8)
        single = wait_selected(device, lambda v: v != army_selected, timeout=3.0)
        device.tap(gx, gy)
        time.sleep(0.9)
        recalled = wait_selected(device, lambda v: v >= 1)
        recall_shot = shot("03-group-recalled.png")
        cycle["steps"]["control_groups"] = {
            "army_button": device.logs().count("BUTTON ARMY") > before_army,
            "selected_after_army": army_selected,
            "slot_rect": [round(v, 1) for v in group[0]],
            "saved_screenshot": rel(saved_shot),
            "selected_after_base_card": single,
            "selected_after_recall": recalled,
            "recall_ok": recalled == army_selected and single == 1,
            "recall_screenshot": rel(recall_shot),
        }

        # 3. minimap: tap a corner, the world view must move -----------------
        pre_minimap = shot("04-before-minimap.png")
        mm = lay["minimap"]
        device.tap(mm[0] + (mm[2] - mm[0]) * 0.22, mm[1] + (mm[3] - mm[1]) * 0.72)
        time.sleep(0.9)
        post_minimap = shot("05-after-minimap.png")
        wr = world_rect(lay, w, h)
        world_before = crop_array(pre_minimap, (wr[0], wr[1], wr[0] + wr[2], wr[1] + wr[3]))
        world_after = crop_array(post_minimap, (wr[0], wr[1], wr[0] + wr[2], wr[1] + wr[3]))
        diff_minimap = mean_abs_diff(world_before, world_after)
        cycle["steps"]["minimap"] = {
            "tap": [
                round(mm[0] + (mm[2] - mm[0]) * 0.22, 1),
                round(mm[1] + (mm[3] - mm[1]) * 0.72, 1),
            ],
            "world_diff": diff_minimap,
            "screenshot_before": rel(pre_minimap),
            "screenshot_after": rel(post_minimap),
            "camera_moved": bool(diff_minimap is not None and diff_minimap > 1.0),
        }

        # 4. tooltip: press-and-hold a card button; the HUD draws the tooltip
        #    in mobile_hud.rs:925-930 (600x30 at console.y-70), a static band.
        s_scale = lay["scale"]
        tooltip_rect = (
            lay["safe_frame"][0] + (lay["safe_frame"][2] - lay["safe_frame"][0]) / 2 - 300.0 * s_scale,
            lay["console"][1] - 70.0 * s_scale,
            lay["safe_frame"][0] + (lay["safe_frame"][2] - lay["safe_frame"][0]) / 2 + 300.0 * s_scale,
            lay["console"][1] - 40.0 * s_scale,
        )
        control_a = shot("06a-tooltip-control-a.png")
        time.sleep(0.4)
        control_b = shot("06b-tooltip-control-b.png")
        control_diff = mean_abs_diff(
            crop_array(control_a, tooltip_rect), crop_array(control_b, tooltip_rect)
        )
        bx, by = centre(slots[2])
        before_tip = shot("06-before-tooltip.png")
        device.su(f"input motionevent DOWN {int(bx)} {int(by)}", timeout=20)
        time.sleep(0.45)
        during_tip = shot("07-tooltip.png")
        device.su(f"input motionevent UP {int(bx)} {int(by)}", timeout=20)
        time.sleep(0.3)
        diff_tip = mean_abs_diff(
            crop_array(before_tip, tooltip_rect), crop_array(during_tip, tooltip_rect)
        )
        visible = bool(
            diff_tip is not None
            and control_diff is not None
            and diff_tip > max(0.8, control_diff * 2.0)
        )
        cycle["steps"]["tooltip"] = {
            "anchor": "command card slot 2 (BASE)",
            "tooltip_rect": [round(v, 1) for v in tooltip_rect],
            "press_diff": diff_tip,
            "control_diff": control_diff,
            "screenshot": rel(during_tip),
            "visible": visible,
        }

        # 5. command card: a known slot must log BUTTON BASE -----------------
        before_base = device.logs().count("BUTTON BASE")
        device.tap(*centre(slots[2]))
        time.sleep(0.7)
        card_shot = shot("08-command-card.png")
        cycle["steps"]["command_card"] = {
            "button_base": device.logs().count("BUTTON BASE") > before_base,
            "screenshot": rel(card_shot),
        }

        # 6. build: worker -> BUILD card -> free ground ----------------------
        device.tap(*centre(slots[2]))  # BASE recentres the camera on the hall
        time.sleep(0.9)
        device.tap(*centre(slots[1]))  # ARMY selects workers among the units
        time.sleep(0.8)
        worker_ready = wait_selected(device, lambda v: v >= 1)
        before_build = device.logs().count("PLACE_BUILT htow")
        before_build_button = device.logs().count("BUTTON BUILD")
        device.tap(*centre(slots[7]))
        time.sleep(0.7)
        build_button_logged = device.logs().count("BUTTON BUILD") > before_build_button
        build_attempts = []
        built = False
        placed_at = None
        wr_place = world_rect(lay, w, h)
        centre_x = wr_place[0] + wr_place[2] / 2.0
        centre_y = wr_place[1] + wr_place[3] / 2.0
        for dx, dy in PLACEMENT_OFFSETS:
            px, py = centre_x + dx, centre_y + dy
            marks_before = {key: device.logs().count(key) for key in ("PLACE_TAP", "PLACE_WORLD", "PLACE_BLOCKED", "PLACE_BUILT")}
            device.tap(px, py)
            time.sleep(0.7)
            logs = device.logs()
            attempts_marks = {key: logs.count(key) - marks_before[key] for key in marks_before}
            build_attempts.append(
                {
                    "point": [round(px, 1), round(py, 1)],
                    "built": "PLACE_BUILT htow cost=" in logs,
                    "blocked": attempts_marks["PLACE_BLOCKED"] > 0,
                    "marks": attempts_marks,
                }
            )
            if "PLACE_BUILT htow cost=" in logs:
                built = True
                placed_at = (px, py)
                break
            if attempts_marks["PLACE_TAP"] == 0 and attempts_marks["PLACE_WORLD"] == 0:
                # the tap was swallowed (HUD rect) or placement mode dropped:
                # re-arm BUILD once and retry the same point
                device.tap(*centre(slots[7]))
                time.sleep(0.5)
                marks_before = {key: logs.count(key) for key in marks_before}
                device.tap(px, py)
                time.sleep(0.7)
                logs = device.logs()
                attempts_marks = {key: logs.count(key) - marks_before[key] for key in marks_before}
                build_attempts.append(
                    {
                        "point": [round(px, 1), round(py, 1)],
                        "rearmed": True,
                        "built": "PLACE_BUILT htow cost=" in logs,
                        "blocked": attempts_marks["PLACE_BLOCKED"] > 0,
                        "marks": attempts_marks,
                    }
                )
                if "PLACE_BUILT htow cost=" in logs:
                    built = True
                    placed_at = (px, py)
                    break
        build_shot = shot("09-after-build.png")
        cycle["steps"]["build"] = {
            "selected": worker_ready,
            "button_build": build_button_logged,
            "attempts": build_attempts,
            "built": built,
            "place_built": next(
                (line for line in device.logs().splitlines() if "PLACE_BUILT htow" in line),
                None,
            ),
            "screenshot": rel(build_shot),
        }

        # 7. train: hall selected by placement, card slot 3 ------------------
        train_ok = False
        train_requested = False
        train_logs: list[str] = []
        button_logs: list[str] = []
        if built:
            if placed_at:
                device.tap(placed_at[0], placed_at[1])
                time.sleep(0.8)
            baseline_units = int(last_state(device.logs()).get("units", 0) or 0)
            before_train = device.logs().count("TRAIN ")
            before_buttons = device.logs().count("BUTTON ")
            device.tap(*centre(slots[3]))
            time.sleep(1.2)
            logs = device.logs()
            train_logs = [line for line in logs.splitlines() if "TRAIN " in line][-4:]
            button_logs = [
                line for line in logs.splitlines() if "BUTTON " in line
            ][-4:] if logs.count("BUTTON ") > before_buttons else []
            train_requested = logs.count("TRAIN ") > before_train
            if train_requested:
                deadline = time.time() + 32
                while time.time() < deadline:
                    if int(last_state(device.logs()).get("units", 0) or 0) > baseline_units:
                        train_ok = True
                        break
                    time.sleep(1.5)
        train_shot = shot("10-after-train.png")
        cycle["steps"]["train"] = {
            "buttons": button_logs,
            "requested": train_requested,
            "ok": train_ok,
            "logs": train_logs,
            "screenshot": rel(train_shot),
        }

        # 8. box drag + getevent --------------------------------------------
        device.su(f"sh -c ': > {GETEVENT_REMOTE}'", timeout=15)
        device.su(f"sh -c 'timeout 10 getevent -lt > {GETEVENT_REMOTE} 2>&1 &'", timeout=15)
        time.sleep(0.5)
        sel_before = selected_now(device)
        before_box = shot("11-box-before.png")
        device.timed_drag(w * 0.42, h * 0.28, w * 0.72, h * 0.48, hold_ms=450, steps=10, step_ms=25)
        time.sleep(2.0)
        getevent = device.su(f"cat {GETEVENT_REMOTE}", timeout=20).stdout or ""
        lines = [line for line in getevent.splitlines() if line.strip()]
        getevent_ok = bool(lines) and "Usage: getevent" not in getevent
        injected = getevent_ok and any(
            marker in getevent for marker in ("EV_ABS", "EV_KEY", "SYN_REPORT")
        )
        sel_after = selected_now(device)
        after_box = shot("12-box-after.png")
        cycle["steps"]["box"] = {
            "selected_before": sel_before,
            "selected_after": sel_after,
            "getevent_lines": len(lines),
            "getevent_ok": getevent_ok,
            "getevent_injected_seen": injected,
            "getevent_head": lines[:30],
            "screenshot_before": rel(before_box),
            "screenshot_after": rel(after_box),
            "classification": "harness_boundary",
        }

        # 9. memory + logcat tail -------------------------------------------
        mem = device.su(f"dumpsys meminfo {PKG}", timeout=30).stdout or ""
        pss = None
        for line in mem.splitlines():
            if line.strip().startswith("TOTAL") and len(line.split()) >= 2:
                parts = line.split()
                if parts[1].isdigit():
                    pss = int(parts[1])
                    break
        interesting = [
            line for line in device.logs().splitlines()
            if "wc3rust" in line and ("GAME_" in line or "PLACE_" in line or "TRAIN " in line or "BUTTON " in line)
        ][-40:]
        cycle["steps"]["runtime"] = {"pss_kb": pss, "log_tail": interesting}

        required = {
            "launch": bool((launch.get("first_frame") or {}).get("first_frame")),
            "control_groups": cycle["steps"]["control_groups"]["recall_ok"],
            "minimap": cycle["steps"]["minimap"]["camera_moved"],
            "command_card": cycle["steps"]["command_card"]["button_base"],
            "build": built,
            "train": train_ok,
            "getevent": injected,
        }
        cycle["required"] = required
        cycle["status"] = "PASS" if all(required.values()) else "FAIL"
    except Exception as error:  # one cycle: record, never retry
        cycle["error"] = f"{type(error).__name__}: {error}"
        cycle["status"] = "FAIL"
    finally:
        cleanup: dict = {}
        try:
            proc = device.su(f"am force-stop {PKG}", timeout=20)
            cleanup["force_stop_rc"] = proc.returncode
            proc = device.su("am start -W -n com.termux/com.termux.app.TermuxActivity", timeout=30)
            cleanup["termux_rc"] = proc.returncode
            deadline = time.time() + 8.0
            while time.time() < deadline:
                if "com.termux" in device.focus():
                    break
                time.sleep(0.6)
            cleanup["focus"] = device.focus()
        except Exception as error:
            cleanup["error"] = f"{type(error).__name__}: {error}"
        cycle["cleanup"] = cleanup

    (out / "cycle.json").write_text(json.dumps(cycle, indent=2) + "\n")
    (out / "getevent.log").write_text(
        "\n".join(cycle.get("steps", {}).get("box", {}).get("getevent_head", [])) + "\n"
    )
    steps = cycle.get("steps", {})
    lines = [
        "# P0 device cycle",
        "",
        f"Timestamp: **{cycle['timestamp']}**",
        f"APK sha256: `{cycle['apk']['sha256']}`",
        f"Status: **{cycle['status']}**",
        "",
        f"- Launch: {(steps.get('launch') or {}).get('first_frame')}",
        f"- Console: height {(steps.get('launch') or {}).get('console_height')} px "
        f"(target {(steps.get('launch') or {}).get('console_target')})",
        f"- Control groups: saved {((steps.get('control_groups') or {}).get('selected_after_army'))}, "
        f"cleared {((steps.get('control_groups') or {}).get('selected_after_clear'))}, "
        f"recalled {((steps.get('control_groups') or {}).get('selected_after_recall'))}",
        f"- Minimap: world diff {(steps.get('minimap') or {}).get('world_diff')} "
        f"-> moved {(steps.get('minimap') or {}).get('camera_moved')}",
        f"- Tooltip: {(steps.get('tooltip') or {}).get('visible')} "
        f"(world diff {(steps.get('tooltip') or {}).get('world_diff')})",
        f"- Command card: BUTTON BASE {(steps.get('command_card') or {}).get('button_base')}",
        f"- Build: {(steps.get('build') or {}).get('built')} {(steps.get('build') or {}).get('place_built')}",
        f"- Train: {(steps.get('train') or {}).get('ok')} {(steps.get('train') or {}).get('logs')}",
        f"- Box: selected {(steps.get('box') or {}).get('selected_before')} -> "
        f"{(steps.get('box') or {}).get('selected_after')}, getevent lines "
        f"{(steps.get('box') or {}).get('getevent_lines')}, injected "
        f"{(steps.get('box') or {}).get('getevent_injected_seen')}",
        f"- PSS: {(steps.get('runtime') or {}).get('pss_kb')} kB",
        "",
        f"Cleanup: {cycle.get('cleanup')}",
        "",
    ]
    (out / "REPORT.md").write_text("\n".join(lines))
    print(json.dumps({"status": cycle["status"], "required": cycle.get("required"), "error": cycle.get("error")}))
    return 0 if cycle["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
