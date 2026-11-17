#!/usr/bin/env python3
"""Operator-bug device scenarios (slot-gated).

Runs the six interaction scenarios the operator's bug list needs to be judged by
eye, in ONE app session, and writes screenshots + a JSON record:

  01-ready        launch, map/scenario lines
  02-select-worker
  03-move         worker ordered across the map, displacement measured
  04-build-menu   worker card with build entries
  05-ghost        press-and-hold placement ghost
  06-place        placed building (foundation/scale)
  07-harvest      worker at a resource, lumber delta
  08-building     building selection (card/portrait)
  09-army-panel   multi-selection panel
  10-card         final card/labels close-up

Device access requires H2_DEVICE_SLOT=1; without it the script exits rc=3 and
writes nothing. It never installs unless the slot is present, and always stops
the app and returns the device to Termux.

Usage: H2_DEVICE_SLOT=1 python3 scripts/qa/operator_scenarios.py [--apk PATH]
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import sys
import time
import traceback
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

OUT = ROOT / "analysis" / "agent-qa" / "operator-bugs"


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


def blue_boxes(image: np.ndarray, lay: dict, w: int, h: int) -> list[list[int]]:
    rect = world_rect(lay, w, h)
    units = analyze.detect_units(image, rect)
    boxes = []
    for cluster in units.get("blue_clusters", []):
        bw = cluster[2] - cluster[0] + 1
        bh = cluster[3] - cluster[1] + 1
        if 4 <= min(bw, bh) and max(bw, bh) <= 140 and cluster[4] >= 8:
            boxes.append(
                [rect[0] + cluster[0], rect[1] + cluster[1], rect[0] + cluster[2], rect[1] + cluster[3]]
            )
    return boxes


def centroid(boxes: list[list[int]]) -> tuple[float, float] | None:
    if not boxes:
        return None
    xs = [(b[0] + b[2]) / 2 for b in boxes]
    ys = [(b[1] + b[3]) / 2 for b in boxes]
    return (sum(xs) / len(xs), sum(ys) / len(ys))


def centre(rect: list[float]) -> tuple[float, float]:
    return ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0)



def button_candidates(lay: dict, w: int, h: int) -> list[tuple[float, float]]:
    """Physical card slot centres plus a margin grid (HUD builds differ)."""
    points: list[tuple[float, float]] = []
    for slot in lay.get("card_slots") or []:
        points.append(centre(slot))
    card = lay.get("card") or [0, 0, 0, 0]
    x0 = card[0]
    y0 = max(0.0, card[1] - 120.0)
    for row in range(4):
        for col in range(4):
            points.append((x0 + 34.0 + col * 68.0, y0 + 40.0 + row * 62.0))
    seen = []
    for point in points:
        if 0 <= point[0] < w and 0 <= point[1] < h and point not in seen:
            seen.append(point)
    return seen


def find_button(device: Device, lay: dict, w: int, h: int, label: str) -> tuple[float, float] | None:
    """Tap candidate card points until logcat reports `BUTTON <label>`."""
    before = device.logs().count(f"BUTTON {label}")
    for point in button_candidates(lay, w, h):
        device.tap(point[0], point[1])
        time.sleep(0.35)
        if device.logs().count(f"BUTTON {label}") > before:
            return point
    return None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--apk", type=Path, default=ROOT / "build" / "android-p0" / "herofolends-p0.apk")
    parser.add_argument("--out", type=Path, default=OUT)
    args = parser.parse_args(argv)
    if args.out.resolve() != OUT:
        args.out = OUT
    out = args.out
    shots = out / "screens"
    shots.mkdir(parents=True, exist_ok=True)

    slot = __import__("os").environ.get("H2_DEVICE_SLOT") == "1"
    if not slot:
        print("DEVICE_SLOT_ABSENT: H2_DEVICE_SLOT != 1 (no device access, nothing written)", file=sys.stderr)
        return 3
    if not args.apk.is_file():
        print(f"APK_MISSING {args.apk}", file=sys.stderr)
        return 2

    device = Device()
    run: dict = {
        "schema": "herofolends-operator-scenarios-v1",
        "timestamp": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "apk": {"path": str(args.apk), "sha256": sha256(args.apk)},
        "steps": {},
        "status": "FAIL",
    }
    try:
        install = device.install(args.apk)
        run["install"] = install
        launch = device.launch(apk=args.apk, install=False)
        display = launch["display"]
        w, h = display["w"], display["h"]
        safe = launch.get("safe_insets") or launch["insets"]
        run["display"] = display
        run["launch"] = {"first_frame": launch.get("first_frame"), "safe_insets": safe}
        lay = {**console_layout.layout(w, h, safe), **(parse_game_hud(device.logs()) or {})}
        slots = lay.get("card_slots") or []
        if len(slots) < 8:
            raise QaError(f"card slots={len(slots)} (need >=8)")

        def shot(name: str) -> Path:
            path = shots / name
            device.screenshot(path, display)
            return path

        def rel(path: Path) -> str:
            return str(path.relative_to(ROOT))

        # 01 ready ---------------------------------------------------------
        ready = shot("01-ready.png")
        state0 = last_state(device.logs())
        logs = device.logs()
        scenario = next((line for line in logs.splitlines() if "GAME_SCENARIO" in line), None)
        ready_line = next((line for line in logs.splitlines() if "GAME_READY" in line), None)
        run["steps"]["ready"] = {
            "screenshot": rel(ready),
            "scenario": scenario,
            "ready": ready_line,
            "state": state0,
        }

        # 02 select worker --------------------------------------------------
        image = np.asarray(Image.open(ready).convert("RGB"))
        boxes = blue_boxes(image, lay, w, h)
        selected = 0
        worker_point = None
        for box in boxes[:12]:
            px, py = centre(box)
            device.tap(px, py)
            time.sleep(0.6)
            selected = int(last_state(device.logs()).get("selected", 0) or 0)
            if selected == 1:
                worker_point = [round(px, 1), round(py, 1)]
                break
        army_point = None
        if selected != 1:
            army_point = find_button(device, lay, w, h, "ARMY")
            time.sleep(0.6)
            selected = int(last_state(device.logs()).get("selected", 0) or 0)
        select_shot = shot("02-select-worker.png")
        run["steps"]["select_worker"] = {
            "army_button": army_point,
            "boxes": len(boxes),
            "tap": worker_point,
            "selected": selected,
            "screenshot": rel(select_shot),
        }

        # 03 move across the map -------------------------------------------
        before_img = np.asarray(Image.open(select_shot).convert("RGB"))
        before_boxes = blue_boxes(before_img, lay, w, h)
        before_centroid = centroid(before_boxes)
        target = (w * 0.72, h * 0.30)
        device.tap(*target)
        time.sleep(2.6)
        move_shot = shot("03-move.png")
        after_img = np.asarray(Image.open(move_shot).convert("RGB"))
        after_centroid = centroid(blue_boxes(after_img, lay, w, h))
        moved = None
        if before_centroid and after_centroid:
            moved = round(float(np.hypot(after_centroid[0] - before_centroid[0],
                                         after_centroid[1] - before_centroid[1])), 1)
        run["steps"]["move"] = {
            "target": [round(target[0], 1), round(target[1], 1)],
            "centroid_before": before_centroid,
            "centroid_after": after_centroid,
            "displacement_px": moved,
            "state": last_state(device.logs()),
            "screenshot": rel(move_shot),
        }

        # 04 build menu -----------------------------------------------------
        before_build = device.logs().count("BUTTON BUILD")
        build_point = find_button(device, lay, w, h, "BUILD")
        time.sleep(0.5)
        build_menu_shot = shot("04-build-menu.png")
        run["steps"]["build_menu"] = {
            "build_button": build_point,
            "button_build_logged": device.logs().count("BUTTON BUILD") > before_build,
            "screenshot": rel(build_menu_shot),
        }

        # 05 ghost (press and hold in the world) ----------------------------
        gx, gy = w * 0.56, h * 0.34
        ghost_before = shot("05a-ghost-before.png")
        device.su(f"input motionevent DOWN {int(gx)} {int(gy)}", timeout=20)
        time.sleep(0.5)
        ghost_shot = shot("05-ghost.png")
        device.su(f"input motionevent UP {int(gx)} {int(gy)}", timeout=20)
        time.sleep(0.4)
        ghost_after = shot("05b-ghost-after.png")
        band = (w * 0.35, h * 0.20, w * 0.75, h * 0.50)
        before_px = np.asarray(Image.open(ghost_before).convert("RGB"))
        during_px = np.asarray(Image.open(ghost_shot).convert("RGB"))
        x0, y0, x1, y1 = (int(v) for v in band)
        diff = float(np.abs(before_px[y0:y1, x0:x1].astype(np.int16)
                            - during_px[y0:y1, x0:x1].astype(np.int16)).mean())
        run["steps"]["ghost"] = {
            "hold_point": [round(gx, 1), round(gy, 1)],
            "region_diff": round(diff, 2),
            "screenshot": rel(ghost_shot),
            "screenshot_after": rel(ghost_after),
        }

        # 06 place ----------------------------------------------------------
        placed = None
        placed_at = None
        for px, py in ((0.58, 0.36), (0.52, 0.40), (0.62, 0.42), (0.48, 0.30)):
            device.tap(w * px, h * py)
            time.sleep(0.7)
            marker = next(
                (line for line in device.logs().splitlines() if "PLACE_BUILT" in line), None
            )
            if marker:
                placed = marker
                placed_at = [round(w * px, 1), round(h * py, 1)]
                break
        place_shot = shot("06-place.png")
        run["steps"]["place"] = {
            "place_built": placed,
            "point": placed_at,
            "state": last_state(device.logs()),
            "screenshot": rel(place_shot),
        }

        # 07 harvest --------------------------------------------------------
        lumber_before = last_state(device.logs()).get("lumber")
        find_button(device, lay, w, h, "ARMY")
        time.sleep(0.7)
        harvest_targets = [(0.30, 0.55), (0.34, 0.62), (0.26, 0.48), (0.38, 0.48)]
        harvest_tap = None
        for px, py in harvest_targets:
            device.tap(w * px, h * py)
            time.sleep(0.6)
            if any("HARVEST" in line or "GATHER" in line for line in device.logs().splitlines()[-40:]):
                harvest_tap = [round(w * px, 1), round(h * py, 1)]
                break
        time.sleep(3.0)
        harvest_shot = shot("07-harvest.png")
        state_harvest = last_state(device.logs())
        run["steps"]["harvest"] = {
            "tap": harvest_tap,
            "lumber_before": lumber_before,
            "lumber_after": state_harvest.get("lumber"),
            "state": state_harvest,
            "screenshot": rel(harvest_shot),
        }

        # 08 building selection --------------------------------------------
        building_shot = shot("08a-building-before.png")
        if placed_at:
            device.tap(placed_at[0], placed_at[1])
            time.sleep(0.8)
        building_shot = shot("08-building.png")
        run["steps"]["building"] = {
            "selected": int(last_state(device.logs()).get("selected", 0) or 0),
            "screenshot": rel(building_shot),
        }

        # 09 army panel -----------------------------------------------------
        find_button(device, lay, w, h, "ARMY")
        time.sleep(0.9)
        army_shot = shot("09-army-panel.png")
        run["steps"]["army_panel"] = {
            "selected": int(last_state(device.logs()).get("selected", 0) or 0),
            "screenshot": rel(army_shot),
        }

        # 10 card close-up --------------------------------------------------
        card_shot = shot("10-card.png")
        run["steps"]["card"] = {"screenshot": rel(card_shot)}

        # logs --------------------------------------------------------------
        logs = device.logs()
        interesting = [
            line for line in logs.splitlines()
            if any(tag in line for tag in (
                "GAME_SCENARIO", "GAME_READY", "GAME_STATE", "GAME_AUDIO_STATS",
                "GAME_ASSET", "GAME_MODEL", "PLACE_", "TRAIN ", "BUTTON ",
            ))
        ]
        (out / "logcat.txt").write_text("\n".join(interesting[-400:]) + "\n")
        (out / "logcat_full.txt").write_text(logs[-400000:])
        run["logs"] = rel(out / "logcat.txt")
        mem = device.su(f"dumpsys meminfo {PKG}", timeout=30).stdout or ""
        pss = None
        for line in mem.splitlines():
            if line.strip().startswith("TOTAL") and len(line.split()) >= 2 and line.split()[1].isdigit():
                pss = int(line.split()[1])
                break
        run["runtime"] = {"pss_kb": pss}
        run["status"] = "PASS" if run["steps"]["ready"].get("state") else "FAIL"
    except Exception as error:  # record once, never retry
        run["error"] = f"{type(error).__name__}: {error}"
        run["traceback"] = traceback.format_exc()[-2000:]
        run["status"] = "FAIL"
    finally:
        cleanup: dict = {}
        try:
            cleanup["force_stop_rc"] = device.su(f"am force-stop {PKG}", timeout=20).returncode
            cleanup["termux_rc"] = device.su(
                "am start -W -n com.termux/com.termux.app.TermuxActivity", timeout=30
            ).returncode
            deadline = time.time() + 8
            while time.time() < deadline:
                if "com.termux" in device.focus():
                    break
                time.sleep(0.5)
            cleanup["focus"] = device.focus()
        except Exception as error:
            cleanup["error"] = f"{type(error).__name__}: {error}"
        run["cleanup"] = cleanup

    (out / "scenarios.json").write_text(json.dumps(run, indent=2) + "\n")
    steps = run.get("steps", {})
    lines = [
        "# Operator-bug device scenarios",
        "",
        f"Timestamp: **{run['timestamp']}**",
        f"APK sha256: `{run['apk']['sha256']}`",
        f"Status: **{run['status']}**",
        "",
        f"- ready: {(steps.get('ready') or {}).get('scenario')}",
        f"- select worker: selected {(steps.get('select_worker') or {}).get('selected')}",
        f"- move: displacement {(steps.get('move') or {}).get('displacement_px')} px",
        f"- build menu: BUTTON BUILD {(steps.get('build_menu') or {}).get('button_build_logged')}",
        f"- ghost: region diff {(steps.get('ghost') or {}).get('region_diff')}",
        f"- place: {(steps.get('place') or {}).get('place_built')}",
        f"- harvest: lumber {(steps.get('harvest') or {}).get('lumber_before')} -> "
        f"{(steps.get('harvest') or {}).get('lumber_after')}",
        f"- building: selected {(steps.get('building') or {}).get('selected')}",
        f"- army panel: selected {(steps.get('army_panel') or {}).get('selected')}",
        f"- PSS: {(run.get('runtime') or {}).get('pss_kb')} kB",
        "",
        f"Cleanup: {run.get('cleanup')}",
        "",
    ]
    (out / "SCENARIOS.md").write_text("\n".join(lines))
    print(json.dumps({"status": run["status"], "steps": {k: (v or {}).get("screenshot") for k, v in steps.items()},
                      "error": run.get("error")}))
    return 0 if run["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
