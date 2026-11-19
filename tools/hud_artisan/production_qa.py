#!/usr/bin/env python3
"""HUD Artisan production QA: contact sheets, state-family metrics and regen queues.

Read-only with respect to the source pack. Reports and preview sheets are written
only beneath the requested output directory.
"""
from __future__ import annotations
import argparse, hashlib, json, math, os, re, tempfile
from pathlib import Path
from typing import Any

from PIL import Image, ImageChops, ImageDraw, ImageStat
import alpha_qa

BG = {
    "dark": (22, 22, 25),
    "light": (226, 222, 211),
    "checker": None,
}
STATE_TOKENS = ("normal", "pressed", "disabled", "autocast", "active", "inactive",
                "hover", "selected", "highlighted", "checked", "focused", "pushed")
MAX_PIXELS = 64_000_000


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def safe_file(root: Path, relative: str) -> Path:
    rel = Path(relative)
    if rel.is_absolute() or not rel.parts or any(x in ("", ".", "..") for x in rel.parts):
        raise ValueError(f"unsafe relative asset path: {relative!r}")
    base = root.resolve()
    current = base
    for part in rel.parts:
        current = current / part
        if current.is_symlink():
            raise ValueError(f"symlink asset path rejected: {relative}")
    result = current.resolve()
    if not result.is_relative_to(base):
        raise ValueError(f"asset escapes pack root: {relative}")
    return result


def checker(size: tuple[int, int], cell: int = 12) -> Image.Image:
    im = Image.new("RGB", size, (210, 210, 210))
    d = ImageDraw.Draw(im)
    for y in range(0, size[1], cell):
        for x in range(0, size[0], cell):
            if (x // cell + y // cell) % 2:
                d.rectangle((x, y, min(x+cell-1, size[0]-1), min(y+cell-1, size[1]-1)),
                            fill=(145, 145, 145))
    return im


def composite(im: Image.Image, mode: str) -> Image.Image:
    rgba = im.convert("RGBA")
    if mode == "checker":
        bg = checker(rgba.size)
    else:
        bg = Image.new("RGB", rgba.size, BG[mode])
    bg.paste(rgba, mask=rgba.getchannel("A"))
    return bg


def state_key(name: str) -> tuple[str, str | None]:
    tokens = name.lower().split("_")
    found = [token for token in STATE_TOKENS if token in tokens]
    if not found:
        return name.lower(), None
    # Only remove one explicit state token. This avoids merging unrelated names.
    token = found[-1]
    tokens.remove(token)
    return "_".join(tokens), token


def nine_slice_metrics(path: Path, insets: dict[str, Any]) -> dict[str, Any]:
    with Image.open(path) as source:
        im = source.convert("RGB")
        width, height = im.size
    left, top, right, bottom = (int(round(float(insets[k]))) for k in ("l", "t", "r", "b"))
    if min(left, top, right, bottom) < 0 or left + right >= width or top + bottom >= height:
        return {"sliceable": False}
    zones = {
        "top_stretch": (left, 0, width-right, top),
        "bottom_stretch": (left, height-bottom, width-right, height),
        "left_stretch": (0, top, left, height-bottom),
        "right_stretch": (width-right, top, width, height-bottom),
    }
    metrics = {}
    for name, box in zones.items():
        x0, y0, x1, y1 = box
        if x1-x0 < 2 or y1-y0 < 2:
            metrics[name] = {"width": max(0,x1-x0), "height": max(0,y1-y0),
                             "mean_neighbor_gradient": None, "rgb_variance": None}
            continue
        region = im.crop(box)
        horizontal = ImageChops.difference(region.crop((1,0,region.width,region.height)),
                                           region.crop((0,0,region.width-1,region.height)))
        vertical = ImageChops.difference(region.crop((0,1,region.width,region.height)),
                                         region.crop((0,0,region.width,region.height-1)))
        hmean = sum(ImageStat.Stat(horizontal).mean) / 3
        vmean = sum(ImageStat.Stat(vertical).mean) / 3
        variance = sum(ImageStat.Stat(region).var) / 3
        metrics[name] = {"width": region.width, "height": region.height,
                         "mean_neighbor_gradient": round((hmean+vmean)/2,4),
                         "rgb_variance": round(variance,4)}
    return {"sliceable": True, "insets": [left,top,right,bottom], "stretch_zones": metrics}


def image_metrics(path: Path) -> dict[str, Any]:
    with Image.open(path) as src:
        src.load()
        if src.format != "PNG" or src.width * src.height > MAX_PIXELS:
            raise ValueError("not PNG or pixel budget exceeded")
        im = src.convert("RGBA")
        alpha = im.getchannel("A")
        bbox = alpha.getbbox()
        return {
            "sha256": digest(path), "width": im.width, "height": im.height,
            "alpha_bbox": list(bbox) if bbox else None,
            "transparent_ratio": round(sum(alpha.histogram()[:1]) / (im.width*im.height), 6),
            "semi_transparent_pixels": sum(alpha.histogram()[1:255]),
        }


def state_pair_metrics(a_path: Path, b_path: Path) -> dict[str, Any]:
    with Image.open(a_path) as a0, Image.open(b_path) as b0:
        a = a0.convert("RGBA").resize((64, 64), Image.Resampling.LANCZOS)
        b = b0.convert("RGBA").resize((64, 64), Image.Resampling.LANCZOS)
    am = a.getchannel("A").point(lambda x: 255 if x >= 32 else 0)
    bm = b.getchannel("A").point(lambda x: 255 if x >= 32 else 0)
    intersection = ImageChops.multiply(am, bm).histogram()[255]
    union = ImageChops.lighter(am, bm).histogram()[255]
    iou = intersection / max(1, union)
    # Compare visible RGB after compositing on a neutral background.
    ac = composite(a, "dark")
    bc = composite(b, "dark")
    diff = ImageChops.difference(ac, bc).convert("L")
    mean_diff = sum(i * n for i, n in enumerate(diff.histogram())) / (255 * 64 * 64)
    return {"silhouette_iou": round(iou, 4), "mean_visible_difference": round(mean_diff, 4)}


def render_contact_sheets(assets: list[dict], pack: Path, out: Path,
                          scales=(1.0, 0.5, 0.25), columns: int = 4) -> list[str]:
    out.mkdir(parents=True, exist_ok=True)
    paths = []
    card_w, image_h, label_h, gap = 560, 190, 48, 12
    for mode in ("dark", "light", "checker"):
        rows = math.ceil(len(assets) / columns)
        board = Image.new("RGB", (columns * card_w, rows * (image_h + label_h + gap)),
                          BG["dark"] if mode == "dark" else (238, 236, 228))
        draw = ImageDraw.Draw(board)
        for index, asset in enumerate(assets):
            x = (index % columns) * card_w
            y = (index // columns) * (image_h + label_h + gap)
            draw.text((x+8, y+4), asset["name"][:72], fill=(245, 230, 195) if mode == "dark" else (25, 25, 25))
            try:
                source = safe_file(pack, asset["file"])
                with Image.open(source) as raw:
                    raw.load()
                    original = raw.convert("RGBA")
                tile_w = (card_w - 32) // len(scales)
                for si, scale in enumerate(scales):
                    w = max(1, round(original.width * scale))
                    h = max(1, round(original.height * scale))
                    preview = original.resize((w, h), Image.Resampling.LANCZOS)
                    preview = composite(preview, mode)
                    preview.thumbnail((tile_w-8, image_h-28), Image.Resampling.LANCZOS)
                    tx = x + 8 + si * tile_w
                    ty = y + 28 + (image_h-28-preview.height)//2
                    board.paste(preview, (tx, ty))
                    draw.text((tx, y+image_h+2), f"{int(scale*100)}% ({w}x{h})",
                              fill=(245,230,195) if mode=="dark" else (25,25,25))
            except Exception as exc:
                draw.text((x+8, y+30), f"ERROR: {str(exc)[:90]}", fill=(220,40,40))
        path = out / f"contact-{mode}.png"
        save_atomic(board, path)
        paths.append(str(path))
    return paths


def save_atomic(image: Image.Image, path: Path) -> None:
    if path.is_symlink():
        raise ValueError(f"refusing symlink output: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temp_name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    os.close(fd)
    temp = Path(temp_name)
    try:
        image.save(temp, format="PNG", optimize=True)
        os.replace(temp, path)
    finally:
        temp.unlink(missing_ok=True)


def run(manifest_path: Path, pack: Path, out: Path, jobs_path: Path | None,
        iou_min: float, diff_min: float, diff_max: float) -> dict[str, Any]:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    assets = manifest.get("assets")
    if not isinstance(assets, list) or not assets:
        raise ValueError("manifest assets must be a non-empty array")
    records, errors = [], []
    alpha_flagged = set()
    for asset in assets:
        try:
            path = safe_file(pack, asset["file"])
            if not path.is_file():
                raise ValueError("file missing")
            record={"name": asset["name"], "file": asset["file"], **image_metrics(path)}
            alpha_record=alpha_qa.inspect_png(path,pack.resolve())
            alpha_record["findings"]=alpha_qa.classify(alpha_record,bool(asset.get("requires_alpha",False)),0.01)
            record["alpha_qa"]=alpha_record
            if alpha_record["findings"]:
                alpha_flagged.add(asset["name"])
            if isinstance(asset.get("insets"),dict):
                record["nine_slice"]=nine_slice_metrics(path,asset["insets"])
            records.append(record)
        except Exception as exc:
            errors.append({"asset": asset.get("name"), "error": str(exc)})
    by_name = {a["name"]: a for a in assets}
    groups: dict[str, list[tuple[str, str]]] = {}
    for asset in assets:
        base, state = state_key(asset["name"])
        if state:
            groups.setdefault(base, []).append((state, asset["name"]))
    state_families, flagged = [], set(alpha_flagged)
    for base, members in sorted(groups.items()):
        if len(members) < 2:
            continue
        pair_rows = []
        for i, (state_a, name_a) in enumerate(members):
            for state_b, name_b in members[i+1:]:
                pa, pb = safe_file(pack, by_name[name_a]["file"]), safe_file(pack, by_name[name_b]["file"])
                try:
                    metrics = state_pair_metrics(pa, pb)
                    reasons = []
                    if metrics["mean_visible_difference"] < diff_min:
                        reasons.append("states_visually_near_identical")
                    if metrics["mean_visible_difference"] > diff_max:
                        reasons.append("state_visual_drift")
                    if metrics["silhouette_iou"] < iou_min:
                        reasons.append("state_silhouette_drift")
                    if reasons:
                        flagged.update((name_a, name_b))
                    pair_rows.append({"a": name_a, "state_a": state_a, "b": name_b,
                                      "state_b": state_b, **metrics, "findings": reasons})
                except Exception as exc:
                    pair_rows.append({"a": name_a, "b": name_b, "findings": [str(exc)]})
                    flagged.update((name_a, name_b))
        state_families.append({"family": base, "states": [s for s, _ in members], "pairs": pair_rows})
    # QA warnings are intentionally conservative; they do not auto-reject visual art.
    for row in records:
        if not row["alpha_bbox"]:
            flagged.add(row["name"])
    regeneration = []
    job_map = {}
    if jobs_path:
        plan = json.loads(jobs_path.read_text(encoding="utf-8"))
        if plan.get("source_manifest_sha256") != hashlib.sha256(manifest_path.read_bytes()).hexdigest():
            raise ValueError("jobs.json manifest hash mismatch")
        job_map = {job.get("asset"): job for job in plan.get("jobs", [])}
    for name in sorted(flagged):
        job=job_map.get(name)
        reasons=[]
        for family_row in state_families:
            for pair in family_row["pairs"]:
                if name in (pair.get("a"),pair.get("b")):
                    reasons.extend(pair.get("findings",[]))
        asset_record=next((row for row in records if row["name"]==name),None)
        if asset_record:
            reasons.extend(asset_record.get("alpha_qa",{}).get("findings",[]))
        regeneration.append({
            "asset": name, "reason": sorted(set(reasons)) or ["QA finding"],
            "job": job,
            "dispatchable": job is not None,
            "note": "Use generate.py repair-plan to create a deterministic full-batch plan; selective dispatch still requires verified prior provenance."
        })
    sheets = render_contact_sheets(assets, pack, out / "contact-sheets")
    return {
        "schema_version": 1, "tool": "HUD Artisan production QA",
        "manifest_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "pack_assets": len(assets), "scanned": len(records), "errors": errors,
        "state_families": state_families, "regeneration_candidates": regeneration,
        "contact_sheets": sheets,
        "thresholds": {"state_iou_min": iou_min, "state_difference_min": diff_min,
                       "state_difference_max": diff_max},
        "summary": {"state_families": len(state_families), "flagged_assets": len(flagged),
                    "read_errors": len(errors), "pass": not errors and not flagged},
    }


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description="HUD Artisan multi-surface visual and state QA")
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--pack", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--jobs", type=Path)
    p.add_argument("--state-iou-min", type=float, default=0.45)
    p.add_argument("--state-difference-min", type=float, default=0.015)
    p.add_argument("--state-difference-max", type=float, default=0.45)
    p.add_argument("--strict", action="store_true")
    a = p.parse_args(argv)
    try:
        if a.out.is_symlink() or (a.out.exists() and not a.out.is_dir()):
            raise ValueError("QA output must be a regular directory path, not a symlink")
        report = run(a.manifest, a.pack, a.out, a.jobs,
                     a.state_iou_min, a.state_difference_min, a.state_difference_max)
        a.out.mkdir(parents=True, exist_ok=True)
        report_path = a.out / "production-qa.json"
        if report_path.is_symlink():
            raise ValueError("QA report output cannot be a symlink")
        fd, tmp_name = tempfile.mkstemp(prefix=".production-qa.", suffix=".tmp", dir=a.out)
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(report, stream, indent=2, ensure_ascii=False)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(tmp_name, report_path)
        print(json.dumps(report["summary"], ensure_ascii=False))
        return 1 if a.strict and not report["summary"]["pass"] else 0
    except (OSError, ValueError, KeyError, TypeError) as exc:
        p.exit(2, f"HUD_ARTISAN_QA_ERROR: {exc}\n")


if __name__ == "__main__":
    raise SystemExit(main())
