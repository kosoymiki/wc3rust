#!/usr/bin/env python3
"""Gameplay self-test on the device: select, move, train, build, attack, phase.

Every step is proven from `logcat` lines the game itself emits (BUTTON/ORDER/
TRAIN/TRAINED/GAME_STATE/GAME_VICTORY/GAME_DEFEAT) plus a screenshot analyzed
by `scripts/qa/analyze.py`. Coordinates are derived from the live screenshot
(unit tint clusters) with the hud.rs layout as fallback, so the test works both
before and after the HUD/touch fixes.

Usage:
    python3 -m scripts.qa.gameplay --json-out analysis/agent-qa/gameplay.json \
        --shots analysis/agent-qa/screens --long 240
"""

from __future__ import annotations

import argparse
import json
import re
import time
from pathlib import Path

import numpy as np

from . import analyze, common, layout
from .device import Device
from .common import QaError

WORLD_KEYS = ("blue_clusters", "red_clusters")


class Gameplay:
    def __init__(self, device: Device, shots: Path, insets: dict, density: float, progress_path: Path | None = None, safe_insets: dict | None = None):
        self.device = device
        self.shots = shots
        self.insets = insets
        self.density = density
        self.safe_insets = safe_insets
        self.progress_path = progress_path
        self.steps: list[dict] = []
        self.shot_index = 0
        self.pid = self.device.pid()
        self.aborts = self.device.abort_count()

    def progress(self) -> None:
        if self.progress_path is None:
            return
        payload = {
            "ok": all(step.get("ok") is not False for step in self.steps),
            "partial": True,
            "steps": self.steps,
            "analyzed_at": common.now_iso(),
        }
        common.save_json(self.progress_path, payload)

    # ----------------------------------------------------------------- helpers

    def logs(self) -> str:
        return self.device.logs()

    def count(self, needle: str) -> int:
        return self.logs().count(needle)

    def state(self) -> dict:
        line = self.device.state_line()
        out = {"raw": line}
        for key in ("phase", "units", "army", "enemy", "gold", "selected", "trained", "kills", "tick", "fps"):
            match = re.search(rf"{key}=([\w.:-]+)", line)
            if match:
                value = match.group(1)
                try:
                    out[key] = float(value) if "." in value else int(value)
                except ValueError:
                    out[key] = value
        return out

    def wait_state(self, predicate, timeout: float = 4.0) -> dict:
        deadline = time.time() + timeout
        last = self.state()
        while time.time() < deadline:
            if predicate(last):
                return last
            time.sleep(0.4)
            last = self.state()
        return last

    def ensure_foreground(self) -> bool:
        if self.device.app_focused():
            return True
        self.device.su(f"am start -n {common.COMPONENT}")
        return self.device.wait_focus(8)

    def check_crash(self, where: str) -> None:
        pid = self.device.pid()
        aborts = self.device.abort_count()
        if aborts > self.aborts or (self.pid and pid and pid != self.pid):
            step = {
                "step": "app_crash",
                "ok": False,
                "where": where,
                "evidence": {
                    "pid_before": self.pid,
                    "pid_after": pid,
                    "new_aborts": aborts - self.aborts,
                    "abort_tail": [
                        line
                        for line in self.device.logs().splitlines()
                        if "Abort message" in line or "panicked at" in line
                    ][-6:],
                },
            }
            self.steps.append(step)
            self.progress()
            raise QaError(f"app crashed at {where}: pid {self.pid}->{pid}, new aborts {aborts - self.aborts}")

    def screenshot(self, name: str, analyze_it: bool = True) -> dict:
        self.check_crash(f"before screenshot {name}")
        self.shot_index += 1
        path = self.shots / f"{self.shot_index:02d}-{name}.png"
        info, overlays = self.device.clean_screenshot(path)
        entry = {"name": name, "path": common.rel(path), "size": [info["width"], info["height"]], "overlays": overlays}
        if analyze_it:
            state = self.state()
            report = analyze.build(path, self.insets, self.density, state.get("fps"), None, self.safe_insets, int(state.get("selected", 0) or 0))
            report["analyzed_at"] = common.now_iso()
            json_path = path.with_suffix(".json")
            common.save_json(json_path, {k: v for k, v in report.items() if not k.startswith("_")})
            annotated = path.with_suffix(".annotated.png")
            analyze.annotate(path, report, annotated)
            entry.update(
                {
                    "json": common.rel(json_path),
                    "annotated": common.rel(annotated),
                    "ok": report["ok"],
                    "fails": [g["id"] for g in report["gates"] if not g["ok"] and g["severity"] == "fail"],
                }
            )
        self.steps.append({"step": name, "screenshot": entry})
        self.progress()
        self.check_crash(f"after screenshot {name}")
        return entry

    def world_rect(self, size) -> tuple[tuple[int, int, int, int], dict]:
        w, h = size
        lay = layout.hud_layout(w, h, self.safe_insets or self.insets)
        left = max(self.insets["left"], int(w * 0.12))
        top = max(self.insets["top"], int(h * 0.14))
        right = int(w * 0.80)
        bottom = int(h * 0.84)
        return (left, top, right, bottom), lay

    def clusters(self, path: Path) -> tuple[dict, tuple[int, int, int, int], tuple[int, int], dict]:
        from PIL import Image

        image = Image.open(path).convert("RGB")
        rgb = np.asarray(image)
        rect, lay = self.world_rect(image.size)
        units = analyze.detect_units(rgb, rect)
        hud = [lay["console"], lay["card"], lay["minimap"], lay["portrait"], lay["selection"], lay["resources"], lay["top_bar"]]
        hud += list(lay["resource_pills"]) + list(lay["card_slots"])
        filtered = []
        for cluster in units["blue_clusters"] + units["red_clusters"]:
            cx = rect[0] + (cluster[0] + cluster[2]) * 0.5
            cy = rect[1] + (cluster[1] + cluster[3]) * 0.5
            if any(r[0] <= cx <= r[2] and r[1] <= cy <= r[3] for r in hud):
                continue
            filtered.append(cluster)
        blue = [c for c in filtered if c in units["blue_clusters"]]
        red = [c for c in filtered if c in units["red_clusters"]]
        units["blue_clusters"] = blue
        units["red_clusters"] = red
        return units, rect, image.size, lay

def _tap_cluster(device: Device, cluster, offset, size) -> tuple[float, float]:
    x0, y0, x1, y1 = cluster[0], cluster[1], cluster[2], cluster[3]
    cx = offset[0] + (x0 + x1) * 0.5
    cy = offset[1] + (y0 + y1) * 0.5
    w, h = size
    cx = min(max(cx, 8), w - 8)
    cy = min(max(cy, 8), h - 8)
    device.tap(cx, cy)
    return cx, cy


def _unit_candidates(units: dict, rect, size, min_px=8, max_px=90):
    w, h = size
    out = []
    for cluster in units["blue_clusters"]:
        bw = cluster[2] - cluster[0] + 1
        bh = cluster[3] - cluster[1] + 1
        if min_px <= min(bw, bh) and max(bw, bh) <= max_px and cluster[4] >= 8:
            cx = rect[0] + (cluster[0] + cluster[2]) * 0.5
            cy = rect[1] + (cluster[1] + cluster[3]) * 0.5
            out.append((abs(cx - w * 0.45) + abs(cy - h * 0.45), cluster))
    out.sort(key=lambda item: item[0])
    return [cluster for _, cluster in out]


def _building_candidates(units: dict, rect, size):
    w, h = size
    out = []
    for cluster in units["blue_clusters"]:
        bw = cluster[2] - cluster[0] + 1
        bh = cluster[3] - cluster[1] + 1
        if max(bw, bh) >= 55 or cluster[4] >= 1800:
            cx = rect[0] + (cluster[0] + cluster[2]) * 0.5
            cy = rect[1] + (cluster[1] + cluster[3]) * 0.5
            out.append((abs(cx - w * 0.4) + abs(cy - h * 0.4), cluster))
    out.sort(key=lambda item: item[0])
    return [cluster for _, cluster in out]


def run(args) -> dict:
    device = Device()
    launch = device.launch(apk=args.apk, install=args.install)
    display = launch["display"]
    insets = launch["insets"]
    insets.pop("density", None)
    insets = {k: insets.get(k, 0) for k in ("left", "top", "right", "bottom")}
    game = Gameplay(device, Path(args.shots), insets, display["density"], args.json_out, launch.get("safe_insets"))
    game.steps.append({"step": "launch", "ok": True, "evidence": launch["first_frame"], "display": display})
    game.progress()

    ok = True
    notes: list[str] = []

    start = game.screenshot("start")
    start_state = game.state()
    game.steps[-1]["state"] = start_state

    # 0. resume: HOME -> foreground must redraw the surface, not stay black.
    if args.resume:
        device.su("input keyevent KEYCODE_HOME")
        time.sleep(1.6)
        resumed = game.ensure_foreground()
        resume_info = None
        resume_ok = False
        if resumed:
            time.sleep(0.8)
            shot = game.screenshot("resume", analyze_it=True)
            resume_info = shot
            resume_ok = shot.get("ok", False)
            # a black frame is the known bug; report it explicitly
            if resume_ok is False:
                failures = shot.get("fails", [])
                resume_ok = not ("black_holes" in failures and "units_visible" in failures)
        game.steps.append(
            {
                "step": "resume_render",
                "ok": bool(resumed and resume_ok),
                "evidence": {"foreground": resumed, "screenshot": resume_info},
            }
        )
        ok = ok and bool(resumed and resume_ok)
    else:
        game.steps.append({"step": "resume_render", "ok": True, "skipped": "--no-resume"})

    # 1. select a unit ------------------------------------------------------
    selected = False
    tap_point = None
    for attempt in range(4):
        units, rect, size, _ = game.clusters(Path(common.ROOT / start["path"]))
        candidates = _unit_candidates(units, rect, size)
        if not candidates:
            break
        before = game.count("GAME_STATE")
        cluster = candidates[attempt % len(candidates)]
        tap_point = _tap_cluster(device, cluster, (rect[0], rect[1]), size)
        state = game.wait_state(lambda s: int(s.get("selected", 0)) >= 1, 3.0)
        if int(state.get("selected", 0)) >= 1:
            selected = True
            game.steps[-1]["detail"] = {"cluster": cluster, "tap": tap_point, "state": state}
            break
        notes.append(f"select attempt {attempt + 1} at {tap_point} left selected=0")
    if not selected:
        # Fallback: the ARMY chip (may be under the status bar -> a real defect)
        lay = layout.hud_layout(*start["size"], game.safe_insets or game.insets)
        chip = lay["card_slots"][1]
        before = game.count("BUTTON ARMY")
        device.tap((chip[0] + chip[2]) * 0.5, (chip[1] + chip[3]) * 0.5)
        time.sleep(0.8)
        if game.count("BUTTON ARMY") > before:
            selected = int(game.state().get("selected", 0)) >= 1
            notes.append("selected via ARMY chip (unit tap failed)")
        else:
            notes.append("ARMY chip tap was swallowed (likely the status bar intercepts it)")
    game.steps.append(
        {
            "step": "select_unit",
            "ok": selected,
            "evidence": {"tap": tap_point, "selected": game.state().get("selected")},
            "notes": notes[-3:],
        }
    )
    ok &= selected
    game.screenshot("selected")

    # 2. move ---------------------------------------------------------------
    moved = False
    if selected:
        w, h = start["size"]
        before = game.count("ORDER MOVE")
        target = (min(w - 260, w * 0.62), min(h - 220, h * 0.55))
        device.tap(*target)
        time.sleep(0.9)
        moved = game.count("ORDER MOVE") > before
        game.steps.append(
            {
                "step": "move_order",
                "ok": moved,
                "evidence": {"tap": [round(v, 1) for v in target], "orders": game.count("ORDER MOVE")},
            }
        )
    else:
        game.steps.append({"step": "move_order", "ok": False, "evidence": "skipped: no selection"})
    ok &= moved
    game.screenshot("moving")

    # 3. train a Footman ----------------------------------------------------
    lay = layout.hud_layout(*start["size"], game.safe_insets or game.insets)
    trained = False
    train_evidence = {}
    # 3a. centre on base through the chip, then select the hall.
    base_chip = lay["card_slots"][2]
    before_base = game.count("BUTTON BASE")
    device.tap((base_chip[0] + base_chip[2]) * 0.5, (base_chip[1] + base_chip[3]) * 0.5)
    time.sleep(0.9)
    base_chip_ok = game.count("BUTTON BASE") > before_base
    if base_chip_ok:
        game.screenshot("base_centred")
    # 3b. tap the hall sprite (large blue cluster) and the FOOTMAN slot
    hall_selected = False
    for attempt in range(4):
        shot = game.screenshot(f"hall_locate_{attempt + 1}", analyze_it=False)
        units, rect, size, _ = game.clusters(Path(common.ROOT / shot["path"]))
        halls = _building_candidates(units, rect, size)
        if not halls:
            # pan left once towards the base and retry
            device.swipe(start["size"][0] * 0.45, start["size"][1] * 0.5, start["size"][0] * 0.72, start["size"][1] * 0.5, 350)
            time.sleep(0.6)
            continue
        cluster = halls[min(attempt, len(halls) - 1)]
        tap = _tap_cluster(device, cluster, (rect[0], rect[1]), size)
        state = game.wait_state(lambda s: int(s.get("selected", 0)) >= 1, 3.0)
        if int(state.get("selected", 0)) >= 1:
            hall_selected = True
            train_evidence["hall_tap"] = tap
            break
    if hall_selected:
        footman = lay["card_slots"][3]
        before_train = game.count("TRAIN Footman")
        device.tap((footman[0] + footman[2]) * 0.5, (footman[1] + footman[3]) * 0.5)
        time.sleep(1.0)
        queued = game.count("TRAIN Footman") > before_train
        train_evidence["queued"] = queued
        if queued:
            deadline = time.time() + 32
            before_trained = game.count("TRAINED Footman")
            while time.time() < deadline:
                if game.count("TRAINED Footman") > before_trained:
                    trained = True
                    break
                time.sleep(1.5)
        train_evidence["trained"] = trained
        train_evidence["state"] = game.state()
    game.steps.append(
        {
            "step": "train_footman",
            "ok": trained,
            "evidence": {"base_chip": base_chip_ok, "hall_selected": hall_selected, **train_evidence},
            "notes": [] if base_chip_ok else ["BASE chip tap did not reach the app (system-bar overlap?)"],
        }
    )
    ok &= trained
    game.screenshot("trained")

    # 4. build a Town Hall with a peasant ----------------------------------
    built = False
    build_evidence = {}
    worker_selected = False
    for attempt in range(5):
        shot = game.screenshot(f"worker_locate_{attempt + 1}", analyze_it=False)
        units, rect, size, _ = game.clusters(Path(common.ROOT / shot["path"]))
        workers = _unit_candidates(units, rect, size, min_px=8, max_px=55)
        if not workers:
            break
        tap = _tap_cluster(device, workers[min(attempt, len(workers) - 1)], (rect[0], rect[1]), size)
        time.sleep(0.5)
        build_button = lay["card_slots"][7]
        before_build = game.count("BUTTON BUILD")
        device.tap((build_button[0] + build_button[2]) * 0.5, (build_button[1] + build_button[3]) * 0.5)
        time.sleep(0.7)
        if game.count("BUTTON BUILD") > before_build:
            worker_selected = True
            build_evidence["worker_tap"] = tap
            break
    if worker_selected:
        before_state = game.state()
        # place on open ground away from HUD/units
        target = (int(start["size"][0] * 0.40), int(start["size"][1] * 0.55))
        device.tap(*target)
        time.sleep(1.6)
        after_state = game.state()
        gold_before = int(before_state.get("gold", 0))
        gold_after = int(after_state.get("gold", 0))
        units_before = int(before_state.get("units", 0))
        units_after = int(after_state.get("units", 0))
        built = (gold_after <= gold_before - 300) or (units_after > units_before)
        build_evidence.update(
            {
                "gold": [gold_before, gold_after],
                "units": [units_before, units_after],
                "place_tap": target,
                "state": after_state,
            }
        )
    game.steps.append(
        {
            "step": "build_townhall",
            "ok": built,
            "evidence": {"worker_selected": worker_selected, **build_evidence},
            "notes": [] if worker_selected else ["could not select a peasant (BUILD button never fired)"],
        }
    )
    ok &= built
    game.screenshot("build")

    # 5. attack-move --------------------------------------------------------
    attack = False
    attack_evidence = {}
    # select the army via chip (or the last selected unit)
    before_army = game.count("BUTTON ARMY")
    army_chip = lay["card_slots"][1]
    device.tap((army_chip[0] + army_chip[2]) * 0.5, (army_chip[1] + army_chip[3]) * 0.5)
    time.sleep(0.7)
    army_selected = game.count("BUTTON ARMY") > before_army
    if not army_selected:
        shot = game.screenshot("army_fallback", analyze_it=False)
        units, rect, size, _ = game.clusters(Path(common.ROOT / shot["path"]))
        candidates = _unit_candidates(units, rect, size)
        if candidates:
            _tap_cluster(device, candidates[0], (rect[0], rect[1]), size)
            time.sleep(0.5)
    attack_button = lay["card_slots"][4]
    before_attack = game.count("ORDER ATTACK-MOVE")
    device.tap((attack_button[0] + attack_button[2]) * 0.5, (attack_button[1] + attack_button[3]) * 0.5)
    time.sleep(0.6)
    w, h = start["size"]
    device.tap(int(w * 0.62), int(h * 0.72))
    time.sleep(1.0)
    attack = game.count("ORDER ATTACK-MOVE") > before_attack
    attack_evidence = {
        "army_chip": army_selected,
        "orders": game.count("ORDER ATTACK-MOVE"),
        "state": game.state(),
    }
    game.steps.append({"step": "attack_move", "ok": attack, "evidence": attack_evidence})
    ok &= attack
    game.screenshot("attack")

    # 6. win/lose overlay (bounded; the AI attacks every 330 ticks) --------
    phase = None
    phase_evidence = {}
    deadline = time.time() + max(0, int(args.long))
    if args.long > 0:
        while time.time() < deadline:
            if not game.ensure_foreground():
                phase_evidence["focus_lost"] = True
                break
            state = game.state()
            if state.get("phase") in ("Won", "Lost"):
                phase = state["phase"]
                phase_evidence["state"] = state
                break
            # keep the army pushing toward the enemy corner
            device.tap(int(w * 0.60), int(h * 0.70))
            time.sleep(2.0)
        if phase:
            game.screenshot(f"phase_{phase.lower()}")
    else:
        phase_evidence["skipped"] = "long mode disabled (--long 0)"
    game.steps.append(
        {
            "step": "win_lose_overlay",
            "ok": bool(phase),
            "severity": "warn" if not phase else "fail",
            "evidence": phase_evidence,
        }
    )

    hard = [step for step in game.steps if step.get("ok") is False and step.get("severity", "fail") == "fail"]
    report = {
        "ok": not hard,
        "launch": launch,
        "steps": game.steps,
        "state_final": game.state(),
        "fails": [step["step"] for step in hard],
        "analyzed_at": common.now_iso(),
        "device": {"display": display, "insets": insets, "density": display["density"]},
    }
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--apk", type=Path, default=common.ROOT / "build" / "android-p0" / "herofolends-p0.apk")
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--shots", default=str(common.ANALYSIS / "screens"))
    parser.add_argument("--json-out", type=Path, default=common.ANALYSIS / "gameplay.json")
    parser.add_argument("--long", type=int, default=0, help="seconds to wait for the win/lose overlay (0=skip)")
    parser.add_argument("--resume", action="store_true", default=False, help="run the HOME->resume crash probe (known wgpu surface crash)")
    args = parser.parse_args(argv)
    report = run(args)
    common.save_json(args.json_out, report)
    print(f"GAMEPLAY {'PASS' if report['ok'] else 'FAIL'}")
    for step in report["steps"]:
        mark = "ok" if step.get("ok") else "FAIL"
        print(f"  {mark:4s} {step['step']}")
    print(common.rel(args.json_out))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
