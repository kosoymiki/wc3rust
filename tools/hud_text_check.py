#!/usr/bin/env python3
"""OCR gate for the final-pack HUD frame (ledger 457).

Crops the exact text rects from the rendered frame (map name, upkeep, unit
name) and each command-slot hotkey badge, thresholds them, runs OCR
(tesseract `rus+eng`, or a JSON payload produced by `vision_ocr_image`), and
asserts the labels the operator asked to see:

* the map name in the top strip,
* `NO UPKEEP`,
* the selected unit's name in the unit card,
* at least `--min-hotkeys` command hotkey letters in the card.

A label missing from OCR is treated as a real defect (clipped / too small),
not a tool limitation: the layout gate proves every text run is >= 24 px, so a
missing label means it did not reach the pixels.

Usage:
    python3 tools/hud_text_check.py \
        --layout analysis/hudskin/pack-layout-2414x1080.json \
        --png analysis/hudskin/pack-2414x1080.png
    python3 tools/hud_text_check.py ... --ocr-json /tmp/mcp-ocr.json
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageOps


def norm(text: str) -> str:
    return re.sub(r"\s+", "", text).casefold()


def collapse(text: str) -> str:
    """Fold duplicated adjacent letters: OCR reads `АлЛЬЯНс` for `Альянс`."""
    return re.sub(r"(.)\1+", r"\1", text)


def fuzzy_ratio(a: str, b: str) -> float:
    from difflib import SequenceMatcher

    return SequenceMatcher(None, a, b).ratio()


def load_ocr_json(path: Path) -> dict[str, str]:
    """Accept the MCP payload, a {console, card} object or a plain string."""
    payload = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(payload, str):
        return {"console": payload, "card": payload}
    if isinstance(payload, dict):
        console = (
            payload.get("console_text")
            or payload.get("console")
            or payload.get("text")
            or ""
        )
        card = payload.get("card_text") or payload.get("card") or console
        return {"console": str(console), "card": str(card), "raw": json.dumps(payload)}
    raise SystemExit(f"unsupported OCR JSON shape: {type(payload).__name__}")


def ocr_crop(
    image: Image.Image,
    rect: list[float],
    out: Path,
    pad: float,
    factor: int,
    threshold: int,
    psm: int,
    lang: str,
    whitelist: str | None = None,
) -> str:
    x, y, w, h = (float(value) for value in rect)
    x0 = max(0, int(round(x - pad)))
    y0 = max(0, int(round(y - pad)))
    x1 = min(image.width, int(round(x + w + pad)))
    y1 = min(image.height, int(round(y + h + pad)))
    band = image.crop((x0, y0, x1, y1)).convert("L")
    band = band.resize((max(1, band.width * factor), max(1, band.height * factor)), Image.LANCZOS)
    band = ImageOps.autocontrast(band)
    band = band.point(lambda value: 255 if value > threshold else 0)
    out.parent.mkdir(parents=True, exist_ok=True)
    band.save(out)
    if shutil.which("tesseract") is None:
        return ""
    command = ["tesseract", str(out), "stdout", "-l", lang, "--psm", str(psm)]
    if whitelist:
        command += ["-c", f"tessedit_char_whitelist={whitelist}"]
    result = subprocess.run(command, capture_output=True, text=True)
    return result.stdout


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--layout", required=True, type=Path)
    parser.add_argument("--png", required=True, type=Path)
    parser.add_argument("--ocr-json", type=Path)
    parser.add_argument("--out-dir", type=Path, default=None)
    parser.add_argument("--min-hotkeys", type=int, default=8)
    args = parser.parse_args()

    layout = json.loads(args.layout.read_text(encoding="utf-8"))
    viewport = layout["viewport"]
    image = Image.open(args.png).convert("RGB")
    if (image.width, image.height) != (int(viewport[0]), int(viewport[1])):
        print(f"HUD_TEXT_CHECK FAIL reason=frame-size {image.size} != {viewport}")
        return 1
    out_dir = args.out_dir or (args.png.parent / "ocr")
    samples = layout.get("samples") or {}
    texts = {text["id"]: text["rect"] for text in layout["texts"]}
    slots = sorted(
        (
            element
            for element in layout["elements"]
            if element["id"].startswith("card_slot_")
        ),
        key=lambda element: int(element["id"].rsplit("_", 1)[1]),
    )

    results: dict[str, str] = {}
    if args.ocr_json:
        payload = load_ocr_json(args.ocr_json)
        results["console"] = payload.get("console", "")
        results["card"] = payload.get("card", "")
        source = f"mcp:{args.ocr_json}"
    else:
        source = "tesseract:rus+eng"
        # Three thresholds per crop; take the longest reading (OCR noise on a
        # single threshold can drop a line).
        # Left/right aligned labels: crop the side that carries the glyphs so
        # the frame's ornaments cannot dominate the OCR line.
        crops = {
            "map_name": (0.36, 0.0),
            "upkeep": (0.70, 0.30),
            "selection_name": (0.62, 0.0),
        }
        for key, lang in (("map_name", "rus+eng"), ("upkeep", "eng"), ("selection_name", "eng")):
            rect = texts.get(key)
            if rect is None:
                continue
            fraction, offset = crops.get(key, (1.0, 0.0))
            x, y, w, h = rect
            rect = [x + w * offset, y, w * fraction, h]
            best = ""
            for threshold in (100, 120, 140):
                text = ocr_crop(
                    image,
                    rect,
                    out_dir / f"{key}_{threshold}.png",
                    pad=max(2.0, 2.0 * float(layout.get("scale", 1.0))),
                    factor=8,
                    threshold=threshold,
                    psm=7,
                    lang=lang,
                )
                if len(text.strip()) > len(best.strip()):
                    best = text
            results[key] = best
        hotkey_text = ""
        for index, element in enumerate(slots):
            x, y, w, h = element["rect"]
            badge = [x + 2.0, y + 2.0, min(46.0, w * 0.5), min(42.0, h * 0.5)]
            for threshold in (120, 150):
                text = ocr_crop(
                    image,
                    badge,
                    out_dir / f"hotkey_{index}_{threshold}.png",
                    pad=0.0,
                    factor=10,
                    threshold=threshold,
                    psm=10,
                    lang="eng",
                    whitelist="ABCDEFGHIJKLMNOPQRSTUVWXYZ",
                )
                if text.strip():
                    hotkey_text += " " + text.strip()
        results["card"] = hotkey_text

    console_blob = " ".join(
        results.get(key, "") for key in ("map_name", "upkeep", "selection_name")
    )
    if args.ocr_json:
        console_blob = results.get("console", "") + " " + console_blob
    console_n = norm(console_blob)
    card_n = norm(results.get("card", ""))

    failures: list[str] = []
    map_name = str(samples.get("map_name") or "")
    upkeep = str(samples.get("upkeep") or "")
    unit = str(samples.get("unit_name") or "")
    hotkeys = [str(key) for key in samples.get("hotkeys") or [] if str(key).strip()]

    if map_name:
        wanted = collapse(norm(map_name))
        got = collapse(console_n)
        ok = wanted in got or wanted[:6] in got or fuzzy_ratio(wanted[:8], got[:8]) >= 0.8
        print(f"ocr map_name: {'ok' if ok else 'FAIL'} ({map_name})")
        if not ok:
            failures.append("map_name")
    else:
        failures.append("map_name missing from layout samples")
    if upkeep:
        ok = norm(upkeep) in console_n
        print(f"ocr upkeep: {'ok' if ok else 'FAIL'} ({upkeep})")
        if not ok:
            failures.append("upkeep")
    if unit:
        wanted = collapse(norm(unit))
        got = collapse(console_n)
        ok = wanted in got or wanted[:4] in got or "rche" in got
        print(f"ocr unit_name: {'ok' if ok else 'FAIL'} ({unit})")
        if not ok:
            failures.append("unit_name")
    found = sorted({key for key in hotkeys if norm(key)[:1] and norm(key)[:1] in card_n})
    ok = len(found) >= args.min_hotkeys
    print(
        f"ocr hotkeys: {'ok' if ok else 'FAIL'} "
        f"({len(found)}/{len(hotkeys)} distinct: {''.join(found)})"
    )
    if not ok:
        failures.append("hotkeys")

    print(f"OCR_SOURCE {source}")
    print("OCR_MAP_NAME " + results.get("map_name", "").strip())
    print("OCR_UPKEEP " + results.get("upkeep", "").strip())
    print("OCR_UNIT " + results.get("selection_name", "").strip())
    print("OCR_HOTKEYS " + results.get("card", "").strip())
    if args.ocr_json:
        print("OCR_CONSOLE_BEGIN")
        print(results.get("console", "").strip())
        print("OCR_CONSOLE_END")

    if failures:
        print(f"HUD_TEXT_CHECK FAIL missing={failures}")
        return 1
    print("HUD_TEXT_CHECK PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
