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
import generate as artisan

BG = {
    "dark": (22, 22, 25),
    "light": (226, 222, 211),
    "checker": None,
}
STATE_TOKENS = ("normal", "pressed", "disabled", "autocast", "active", "inactive",
                "hover", "selected", "highlighted", "checked", "focused", "pushed",
                "critical", "warning", "warn", "cooldown", "alert", "empty")
MAX_PIXELS = 64_000_000


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def safe_file(root: Path, relative: str) -> Path:
    if not isinstance(relative,str) or not relative or "\\" in relative or "\x00" in relative:
        raise ValueError(f"unsafe relative asset path: {relative!r}")
    rel = Path(relative)
    if rel.is_absolute() or not rel.parts or any(x in ("", ".", "..") for x in rel.parts):
        raise ValueError(f"unsafe relative asset path: {relative!r}")
    if root.is_symlink():
        raise ValueError(f"pack root must not be a symlink: {root}")
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


def ensure_output_dir(path: Path) -> Path:
    if path.is_symlink():
        raise ValueError(f"symlink output directory rejected: {path}")
    missing=[]
    cursor=path
    while not cursor.exists():
        missing.append(cursor)
        cursor=cursor.parent
    if cursor.is_symlink() or not cursor.is_dir():
        raise ValueError(f"output ancestor must be a real directory: {cursor}")
    for directory in reversed(missing):
        directory.mkdir(exist_ok=True)
        if directory.is_symlink() or not directory.is_dir():
            raise ValueError(f"unsafe output directory component: {directory}")
    if not path.is_dir():
        raise ValueError(f"output path is not a directory: {path}")
    return path.resolve()


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
    token = next((value for value in reversed(tokens) if value in STATE_TOKENS), None)
    if token is None:
        return name.lower(), None
    # Remove the right-most state token: asset naming convention is base_state.
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
    if out.is_symlink() or (out.exists() and not out.is_dir()):
        raise ValueError("contact-sheet output directory must be a regular directory")
    ensure_output_dir(out)
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
    ensure_output_dir(path.parent)
    cursor=path.parent
    while cursor != cursor.parent:
        if cursor.is_symlink():
            raise ValueError(f"refusing symlink output directory: {cursor}")
        cursor=cursor.parent
    fd, temp_name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    os.close(fd)
    temp = Path(temp_name)
    try:
        image.save(temp, format="PNG", optimize=True)
        os.replace(temp, path)
    finally:
        temp.unlink(missing_ok=True)


def render_asset_thumbnails(assets: list[dict], pack: Path, out: Path) -> dict[str,str]:
    if out.is_symlink() or (out.exists() and not out.is_dir()):
        raise ValueError("thumbnail output directory must be a regular directory")
    ensure_output_dir(out)
    result={}
    for asset in assets:
        name=asset.get("name")
        try:
            source=safe_file(pack,asset["file"])
            with Image.open(source) as raw:
                raw.load()
                tile=Image.new("RGBA",(176,176),(0,0,0,0))
                image=raw.convert("RGBA")
                image.thumbnail((160,160),Image.Resampling.LANCZOS)
                tile.alpha_composite(image,((176-image.width)//2,(176-image.height)//2))
            filename=hashlib.sha256(str(name).encode("utf-8")).hexdigest()[:12]+".png"
            path=out/filename
            save_atomic(composite(tile,"checker"),path)
            result[name]=path.relative_to(out.parent).as_posix()
        except Exception:
            continue
    return result


def write_review_gallery(records: list[dict], thumbnails: dict[str,str], sheets: list[str],
                         atlas_report: dict, report: dict, out: Path) -> str:
    import html
    ensure_output_dir(out)
    summary=report.get("summary",{})
    images=[]
    for record in records:
        name=str(record.get("name",""))
        alpha=record.get("alpha_qa",{})
        findings=alpha.get("findings",[])
        thumb=thumbnails.get(name)
        if not thumb:
            continue
        findings_html=", ".join(html.escape(str(x)) for x in findings) or "no alpha flags"
        images.append(
            '<article class="asset" data-name="'+html.escape(name,quote=True).lower()+'">'
            '<img loading="lazy" src="'+html.escape(thumb,quote=True)+'" alt="'+html.escape(name,quote=True)+'">'
            '<h3>'+html.escape(name)+'</h3>'
            '<p>'+html.escape(str(record.get("width","?")))+' × '+html.escape(str(record.get("height","?")))+'</p>'
            '<p>transparent '+html.escape(str(alpha.get("alpha",{}).get("transparent_ratio","?")))+
            ' · semi '+html.escape(str(alpha.get("alpha",{}).get("semi_transparent_ratio","?")))+
            '</p><p class="finding">'+findings_html+'</p></article>'
        )
    sheet_html="".join('<figure><img loading="lazy" src="'+html.escape(Path(path).relative_to(out).as_posix(),quote=True)+
                       '" alt="'+html.escape(Path(path).stem,quote=True)+'"><figcaption>'+
                       html.escape(Path(path).stem)+'</figcaption></figure>' for path in sheets)
    atlas_sheet=atlas_report.get("contact_sheet")
    if atlas_sheet:
        sheet_html+=(
            '<figure><img loading="lazy" src="'+html.escape(Path(atlas_sheet).relative_to(out).as_posix(),quote=True)+
            '" alt="Atlas cells"><figcaption>Atlas cells</figcaption></figure>'
        )
    document='''<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>HUD Artisan — Visual QA</title>
<style>
:root{color-scheme:dark;background:#171719;color:#eee;font:14px system-ui,sans-serif}
body{max-width:1440px;margin:auto;padding:24px}h1,h2{color:#e9c98d}
.summary{display:flex;gap:18px;flex-wrap:wrap;padding:14px;background:#252529;border-radius:10px}
.sheets{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:14px}
figure{margin:0;background:#252529;padding:8px;border-radius:8px}figure img{width:100%;height:auto}
.gallery{display:grid;grid-template-columns:repeat(auto-fill,minmax(190px,1fr));gap:12px}
.asset{background:#252529;border-radius:8px;padding:10px;min-width:0}
.asset img{display:block;width:176px;height:176px;max-width:100%;object-fit:contain;margin:auto}
.asset h3{font-size:14px;overflow-wrap:anywhere}.asset p{margin:5px 0;color:#bbb}
.finding{color:#efbd72!important}.search{width:min(420px,100%);padding:10px;border-radius:7px;border:1px solid #555;background:#222;color:#fff}
</style>
<body><h1>HUD Artisan — Visual QA</h1>
<div class="summary"><span>Assets: '''+html.escape(str(report.get("scanned",0)))+'''</span>
<span>Read errors: '''+html.escape(str(len(report.get("errors",[]))))+'''</span>
<span>State families: '''+html.escape(str(summary.get("state_families",0)))+'''</span>
<span>Flagged: '''+html.escape(str(summary.get("flagged_assets",0)))+'''</span>
<span>Atlas cells: '''+html.escape(str(atlas_report.get("scanned",0)))+'''</span></div>
<h2>Full-pack contact sheets</h2><div class="sheets">'''+sheet_html+'''</div>
<h2>Asset gallery</h2><input class="search" id="filter" placeholder="Filter assets by name" autocomplete="off">
<div class="gallery" id="gallery">'''+''.join(images)+'''</div>
<script>
const input=document.getElementById('filter');
input.addEventListener('input',()=>{const q=input.value.toLowerCase();
document.querySelectorAll('.asset').forEach(card=>{card.hidden=!card.dataset.name.includes(q)})});
</script></body></html>
'''
    path=out/"review.html"
    if path.is_symlink():
        raise ValueError("review gallery output must not be a symlink")
    fd,temp_name=tempfile.mkstemp(prefix=".review.",suffix=".tmp",dir=out)
    try:
        with os.fdopen(fd,"w",encoding="utf-8") as stream:
            stream.write(document);stream.flush();os.fsync(stream.fileno())
        os.replace(temp_name,path)
    finally:
        Path(temp_name).unlink(missing_ok=True)
    return str(path)


def atlas_qa(manifest: dict, pack: Path, out: Path, jobs_plan: dict | None) -> dict[str, Any]:
    atlas_spec=manifest.get("atlas",{}).get("commandbuttons",{})
    if not atlas_spec:
        return {"status":"not_configured","reason":"manifest has no commandbuttons atlas","cells":[],"findings":[]}
    atlas_file=atlas_spec.get("file")
    entries=atlas_spec.get("entries",[])
    if not isinstance(atlas_file,str) or not isinstance(entries,list) or not entries:
        return {"status":"error","reason":"manifest atlas.commandbuttons requires a file and non-empty entries array","cells":[],"findings":[]}
    declared_size=atlas_spec.get("size")
    if declared_size is not None and (not isinstance(declared_size,list) or len(declared_size)!=2
        or any(type(value) is not int or value<=0 for value in declared_size)):
        return {"status":"error","reason":"atlas size must be two positive integers","file":atlas_file,"cells":[],"findings":[]}
    if len(entries)>256:
        return {"status":"error","reason":"atlas entry count exceeds safety limit (256)","file":atlas_file,"cells":[],"findings":[]}
    try:
        atlas_path=safe_file(pack,atlas_file)
        with Image.open(atlas_path) as source:
            if source.width<1 or source.height<1 or source.width*source.height>MAX_PIXELS:
                raise ValueError("atlas dimensions exceed pixel budget")
            source.load()
            if source.format!="PNG":
                raise ValueError("atlas is not PNG")
            if declared_size is not None and [source.width,source.height] != declared_size:
                raise ValueError(f"atlas dimensions {source.width}x{source.height} differ from manifest {declared_size}")
            atlas=source.convert("RGBA")
    except Exception as exc:
        return {"status":"error","reason":str(exc),"file":atlas_file,"cells":[],"findings":[]}
    jobs_by_output={}
    if jobs_plan and isinstance(jobs_plan.get("jobs"),list):
        jobs_by_output={job.get("output"):job for job in jobs_plan["jobs"] if isinstance(job,dict)}
    planned_cells={}
    if jobs_plan and isinstance(jobs_plan.get("atlas"),dict):
        planned_cells={cell.get("id"):cell for cell in jobs_plan["atlas"].get("cells",[]) if isinstance(cell,dict)}
    manifest_assets={asset.get("name"):asset for asset in manifest.get("assets",[]) if isinstance(asset,dict)}
    expected_cell=atlas_spec.get("cell")
    if expected_cell is not None and (not isinstance(expected_cell,list) or len(expected_cell)!=2
        or any(type(value) is not int or value<=0 for value in expected_cell)):
        return {"status":"error","reason":"atlas cell must be two positive integers","file":atlas_file,"cells":[],"findings":[]}
    cells=[];findings=[];columns=4;card_w=184;card_h=208
    rows=math.ceil(len(entries)/columns)
    board=Image.new("RGB",(columns*card_w,max(1,rows)*card_h),(34,34,38))
    draw=ImageDraw.Draw(board)
    for index,entry in enumerate(entries):
        if not isinstance(entry,dict):
            findings.append({"id":f"entry-{index}","reason":"malformed_atlas_entry"})
            continue
        ident=str(entry.get("id",f"entry-{index}"))
        try:
            raw=(entry.get("x"),entry.get("y"),entry.get("w"),entry.get("h"))
            if any(type(value) is not int for value in raw):
                raise ValueError("cell coordinates and dimensions must be integers")
            x,y,w,h=raw
            if not ident or min(x,y)<0 or min(w,h)<=0 or x+w>atlas.width or y+h>atlas.height:
                raise ValueError("cell outside atlas bounds or invalid ID")
            if expected_cell is not None and [w,h] != expected_cell:
                raise ValueError(f"cell dimensions {w}x{h} differ from declared cell {expected_cell}")
            for previous in cells:
                px,py,pw,ph=(previous[k] for k in ("x","y","width","height"))
                if x<px+pw and px<x+w and y<py+ph and py<y+h:
                    raise ValueError(f"cell overlaps previously declared cell {previous['id']}")
            if any(cell["id"]==ident for cell in cells):
                raise ValueError("duplicate atlas cell ID")
            crop=atlas.crop((x,y,x+w,y+h))
            alpha=crop.getchannel("A");hist=alpha.histogram();total=w*h
            bbox=alpha.getbbox()
            sample=crop.resize((min(32,w),min(32,h)),Image.Resampling.LANCZOS)
            colors=len(set(sample.getdata()))
            cell={"id":ident,"x":x,"y":y,"width":w,"height":h,
                  "transparent_ratio":round(hist[0]/total,6),
                  "semi_transparent_pixels":sum(hist[1:255]),
                  "opaque_ratio":round(hist[255]/total,6),
                  "alpha_bbox":list(bbox) if bbox else None,
                  "sample_unique_rgba":colors,
                  "sha256":hashlib.sha256(crop.tobytes()).hexdigest()}
            planned=planned_cells.get(ident,{})
            source_file=planned.get("source_file")
            source_asset=entry.get("source_asset") or entry.get("asset")
            if not source_file and isinstance(source_asset,str):
                source_record=manifest_assets.get(source_asset)
                if source_record:
                    source_file=source_record.get("file")
                    cell["source_asset"]=source_asset
            job=jobs_by_output.get(source_file)
            if job:
                cell["job_asset"]=job.get("asset")
                cell["job_id"]=job.get("job_id")
            elif isinstance(source_asset,str) and source_asset in manifest_assets:
                cell["job_asset"]=source_asset
            if bbox is None:
                cell["findings"]=["fully_transparent_atlas_cell"]
                findings.append({"id":ident,"reason":"fully_transparent_atlas_cell",
                                 "asset":cell.get("job_asset"),"job_id":cell.get("job_id")})
            else:
                cell["findings"]=[]
            cells.append(cell)
            cx=(index%columns)*card_w;cy=(index//columns)*card_h
            draw.text((cx+8,cy+6),ident[:26],fill=(245,230,195))
            preview=composite(crop,"checker")
            preview.thumbnail((152,152),Image.Resampling.NEAREST)
            board.paste(preview,(cx+16+(152-preview.width)//2,cy+36+(152-preview.height)//2))
            draw.text((cx+8,cy+190),f"{w}x{h}  alpha {hist[0]/total:.2f}",
                      fill=(235,235,235))
        except Exception as exc:
            findings.append({"id":ident,"reason":str(exc)})
            cx=(index%columns)*card_w;cy=(index//columns)*card_h
            draw.text((cx+8,cy+8),ident[:26],fill=(255,220,180))
            draw.text((cx+8,cy+34),str(exc)[:26],fill=(255,80,80))
    sheet_path=out/"contact-sheets"/"atlas-cells.png"
    if entries:
        save_atomic(board,sheet_path)
    return {"status":"ok","file":atlas_file,"dimensions":[atlas.width,atlas.height],
            "entry_count":len(entries),"scanned":len(cells),"findings":findings,
            "cells":cells,"contact_sheet":str(sheet_path) if entries else None}


def run(manifest_path: Path, pack: Path, out: Path, jobs_path: Path | None,
        iou_min: float, diff_min: float, diff_max: float) -> dict[str, Any]:
    if not math.isfinite(iou_min) or not 0.0 <= iou_min <= 1.0:
        raise ValueError("state_iou_min must be finite and between 0 and 1")
    if not math.isfinite(diff_min) or not math.isfinite(diff_max) or not 0.0 <= diff_min <= diff_max <= 1.0:
        raise ValueError("state difference thresholds must satisfy 0 <= min <= max <= 1")
    if manifest_path.is_symlink() or not manifest_path.is_file():
        raise ValueError("manifest must be a regular non-symlink file")
    if pack.is_symlink() or not pack.is_dir():
        raise ValueError("pack root must be a regular non-symlink directory")
    ensure_output_dir(out)
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    assets = manifest.get("assets")
    if not isinstance(assets, list) or not assets:
        raise ValueError("manifest assets must be a non-empty array")
    names=set();files=set()
    for index,asset in enumerate(assets):
        if not isinstance(asset,dict) or not isinstance(asset.get("name"),str) or not asset["name"].strip():
            raise ValueError(f"manifest assets[{index}] requires a non-empty name")
        if asset["name"] in names or not isinstance(asset.get("file"),str) or asset["file"] in files:
            raise ValueError(f"duplicate asset name or file at manifest index {index}")
        names.add(asset["name"]);files.add(asset["file"])
    records, errors = [], []
    alpha_flagged = set()
    for asset in assets:
        try:
            path = safe_file(pack, asset["file"])
            if not path.is_file():
                raise ValueError("file missing")
            record={"name": asset["name"], "file": asset["file"], **image_metrics(path)}
            if record["width"] != asset.get("w") or record["height"] != asset.get("h"):
                raise ValueError(f"PNG dimensions {record['width']}x{record['height']} differ from manifest {asset.get('w')}x{asset.get('h')}")
            alpha_record=alpha_qa.inspect_png(path,pack.resolve())
            kind=artisan.family(asset["name"])
            alpha_required=artisan.requires_alpha(kind) or bool(asset.get("requires_alpha",False))
            alpha_record["findings"]=alpha_qa.classify(alpha_record,alpha_required,0.01)
            if kind in ("portrait-frame","minimap-frame"):
                with Image.open(path) as aperture_image:
                    aperture=artisan.center_transparent_fraction(aperture_image.convert("RGBA"))
                alpha_record["center_aperture_transparent_ratio"]=round(aperture,6)
                if aperture < 0.97:
                    alpha_record["findings"].append("center_aperture_not_transparent")
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
    plan = None
    if jobs_path:
        plan = json.loads(jobs_path.read_text(encoding="utf-8"))
        if plan.get("source_manifest_sha256") != hashlib.sha256(manifest_path.read_bytes()).hexdigest():
            raise ValueError("jobs.json manifest hash mismatch")
        job_map = {job.get("asset"): job for job in plan.get("jobs", [])}
    atlas_report=atlas_qa(manifest,pack,out,plan)
    if atlas_report.get("status") not in ("ok","not_configured"):
        errors.append({"asset":"commandbuttons_atlas","error":atlas_report.get("reason","atlas QA unavailable")})
    atlas_findings=atlas_report.get("findings",[])
    for finding in atlas_findings:
        if finding.get("asset"):
            flagged.add(finding["asset"])
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
        for cell in atlas_report.get("cells",[]):
            if cell.get("job_asset")==name:
                reasons.extend(cell.get("findings",[]))
        regeneration.append({
            "asset": name, "reason": sorted(set(reasons)) or ["QA finding"],
            "job": job,
            "dispatchable": job is not None,
            "note": "Use generate.py repair-plan to create a deterministic full-batch plan; selective dispatch still requires verified prior provenance."
        })
    sheets = render_contact_sheets(assets, pack, out / "contact-sheets")
    thumbnails=render_asset_thumbnails(assets,pack,out/"thumbnails")
    gallery=write_review_gallery(records,thumbnails,sheets,atlas_report,
        {"scanned":len(records),"errors":errors,"summary":{"state_families":len(state_families),
         "flagged_assets":len(flagged),"atlas_findings":len(atlas_findings)}},out)
    return {
        "schema_version": 1, "tool": "HUD Artisan production QA",
        "manifest_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "pack_assets": len(assets), "scanned": len(records), "errors": errors,
        "assets": records, "atlas": atlas_report,
        "state_families": state_families, "regeneration_candidates": regeneration,
        "contact_sheets": sheets, "review_gallery": gallery,
        "thresholds": {"state_iou_min": iou_min, "state_difference_min": diff_min,
                       "state_difference_max": diff_max},
        "summary": {"state_families": len(state_families), "flagged_assets": len(flagged),
                    "atlas_findings": len(atlas_findings),
                    "read_errors": len(errors), "pass": not errors and not flagged and not atlas_findings},
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
        ensure_output_dir(a.out)
        report_path = a.out / "production-qa.json"
        if report_path.is_symlink():
            raise ValueError("QA report output cannot be a symlink")
        fd, tmp_name = tempfile.mkstemp(prefix=".production-qa.", suffix=".tmp", dir=a.out)
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                json.dump(report, stream, indent=2, ensure_ascii=False)
                stream.write("\n")
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(tmp_name, report_path)
        finally:
            Path(tmp_name).unlink(missing_ok=True)
        print(json.dumps(report["summary"], ensure_ascii=False))
        return 1 if a.strict and not report["summary"]["pass"] else 0
    except (OSError, ValueError, KeyError, TypeError) as exc:
        p.exit(2, f"HUD_ARTISAN_QA_ERROR: {exc}\n")


if __name__ == "__main__":
    raise SystemExit(main())
