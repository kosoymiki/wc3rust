#!/usr/bin/env python3
"""HUD Artisan 1.0: manifest-driven art production, ingestion and quality gates.

The tool is provider-neutral: an image model or agent consumes jobs.json and writes
one PNG per job into a drop directory. Python stdlib handles planning and PNG integrity;
Pillow provides decode, normalization and pixel-level QA.
"""
from __future__ import annotations
import argparse, hashlib, json, math, re, shutil, struct, sys, zlib
from collections import defaultdict
from pathlib import Path
from typing import Any

VERSION = "1.0.0"
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
MAX_PIXELS = 64_000_000
REQUIRED = ("design_height","reference_resolution","safe_area_1080p","command_card","control_groups","atlas","assets","font_files")
ENGINE_ROLES = [
"console_left_cap_normal_256x288","console_left_mid_normal_256x288","console_right_mid_normal_256x288","console_right_cap_normal_256x288",
"topbar_plate_normal_720x32","menu_panel_normal_512x384","resource_pill_normal_236x32","portrait_frame_normal_128",
"build_progress_normal_64x8","tooltip_panel_normal_384x256","button_border_normal_64","button_glow_64","button_border_pressed_64",
"button_border_autocast_64","button_border_disabled_64","button_border_empty_64","control_group_button_active_44","control_group_button_inactive_44",
"selection_panel_normal_240x180","menu_button_normal_256x64","score_panel_normal_1024x512","menu_button_pressed_256x64",
"loading_frame_normal_1024x512","minimap_frame_normal_216"]
THEMES = {
"classic-modern":{"palette":"aged bronze, hammered iron, warm dark stone, restrained emerald enamel and parchment highlights",
 "materials":"layered forged metal, brushed bronze bevels, worn stone/leather, dark inset surfaces and directional warm light",
 "ornament":"restrained high-fantasy geometry, asymmetrical hand-forged cornerwork, engraved micro-lines and occasional green gems"},
"human-gold":{"palette":"warm gold, pale steel, cream parchment, deep navy and restrained cobalt enamel",
 "materials":"polished steel, brushed gold, worn leather and parchment","ornament":"disciplined heraldic geometry, fine engraved rays and small blue gems"},
"orc-iron":{"palette":"dark iron, oxidized bronze, muted blood red, leather brown and bone",
 "materials":"scarred iron plates, hammered bronze, rough leather and chipped edges","ornament":"heavy angular construction, rivets and blunt forged corners"},
"undead-obsidian":{"palette":"obsidian, tarnished silver, desaturated violet, bone and cold teal",
 "materials":"dark glassy stone, tarnished silver, cold enamel and bone inlay","ornament":"thin gothic spikes, carved abstract geometry and cold luminous accents"}}

def sha(data:bytes)->str: return hashlib.sha256(data).hexdigest()
def slug(s:str)->str: return re.sub(r"[^a-z0-9_-]+","-",s.lower()).strip("-") or "asset"
def safe_rel(value:str)->Path:
    p=Path(value)
    if not value or p.is_absolute() or ".." in p.parts or p.suffix.lower()!=".png":
        raise ValueError(f"unsafe/non-PNG asset path: {value!r}")
    return p
def read_json(path:Path)->dict[str,Any]:
    try: obj=json.loads(path.read_text(encoding="utf-8"))
    except (OSError,json.JSONDecodeError) as e: raise ValueError(f"cannot read {path}: {e}") from e
    if not isinstance(obj,dict): raise ValueError(f"{path}: JSON root must be an object")
    return obj

def load_manifest(path:Path,engine_contract:bool=False)->dict[str,Any]:
    doc=read_json(path); missing=[k for k in REQUIRED if k not in doc]
    if missing: raise ValueError("manifest missing: "+", ".join(missing))
    if not isinstance(doc["assets"],list) or not doc["assets"]: raise ValueError("assets must be a non-empty array")
    names=set(); files=set()
    for i,a in enumerate(doc["assets"]):
        if not isinstance(a,dict): raise ValueError(f"assets[{i}] must be an object")
        for k in ("name","file","w","h","insets"):
            if k not in a: raise ValueError(f"assets[{i}] missing {k}")
        name=a["name"]
        if not isinstance(name,str) or not name.strip() or name in names: raise ValueError(f"invalid/duplicate asset name: {name!r}")
        names.add(name); rel=safe_rel(a["file"])
        if str(rel) in files: raise ValueError(f"duplicate asset file: {rel}")
        files.add(str(rel)); w,h=a["w"],a["h"]
        if type(w)!=int or type(h)!=int or w<1 or h<1 or w*h>MAX_PIXELS: raise ValueError(f"{name}: invalid dimensions {w}x{h}")
        ins=a["insets"]
        if not isinstance(ins,dict) or any(k not in ins for k in ("l","t","r","b")): raise ValueError(f"{name}: insets need l,t,r,b")
        vals=[float(ins[k]) for k in ("l","t","r","b")]
        if any(not math.isfinite(v) or v<0 for v in vals): raise ValueError(f"{name}: insets must be finite and non-negative")
    atlas=doc["atlas"].get("commandbuttons") if isinstance(doc["atlas"],dict) else None
    if not isinstance(atlas,dict) or not isinstance(atlas.get("entries"),list) or not atlas["entries"]:
        raise ValueError("atlas.commandbuttons.entries must be a non-empty array")
    if not isinstance(doc["font_files"],list) or len(doc["font_files"])!=5:
        raise ValueError("wc3rust pack loader requires exactly five font_files")
    if engine_contract:
        absent=sorted(set(ENGINE_ROLES)-names)
        if absent: raise ValueError("required engine roles missing: "+", ".join(absent))
    return doc

def family(name:str)->str:
    n=name.lower()
    if any(x in n for x in ("portrait","hero_frame","unit_frame")): return "portrait-frame"
    if "minimap" in n: return "minimap-frame"
    if n.startswith(("icon_","ability_","item_","spell_","command_")): return "icon"
    if any(x in n for x in ("cursor","reticle","rally_point","order_marker")): return "cursor"
    if any(x in n for x in ("button","slot","tile","tab","control_group")): return "interactive-control"
    if any(x in n for x in ("topbar","console","resource","upkeep","progress","hp_bar","mana_bar")): return "hud-strip"
    if any(x in n for x in ("menu","dialog","modal","loading","score","campaign","save")): return "menu-panel"
    if any(x in n for x in ("tooltip","selection","inventory","panel","frame")): return "panel"
    return "misc-ui"

def material(name:str,kind:str)->str:
    n=name.lower()
    if kind=="icon": return "sculpted miniature emblem with a distinct silhouette, strong value grouping and a controlled metallic rim"
    if kind=="portrait-frame": return "deep dark portrait aperture, thick forged surround, protected inner bevel; centre stays empty"
    if "wood" in n or "lumber" in n: return "carved dark wood with cut end grain and warm sap highlights"
    if "gold" in n or "resource" in n: return "brushed gold, dark iron backing and a clean embossed resource insignia"
    if "console" in n or "topbar" in n: return "broad dark iron plate, substantial bronze rim, low-frequency texture and stretch-safe centre"
    if kind=="menu-panel": return "layered dark leather and aged stone inset, thick bronze/steel outer frame and restrained parchment accents"
    return "dark slate inset, forged metal frame, bronze bevels, engraved detail and restrained enamel accents"

def make_job(a:dict,theme:str,seed:int)->dict:
    name,w,h=a["name"],a["w"],a["h"];kind=family(name);t=THEMES[theme];ins=a["insets"]
    states=", ".join(a.get("states") or []) or "normal/default state"
    alpha=kind in ("icon","cursor")
    prompt=(f"Create ONE production game UI asset named {name}; exact canvas {w}x{h}, ratio {w/h:.5f}. "
    f"Family {kind}; state {states}. Palette: {t['palette']}. Materials: {material(name,kind)}. "
    f"Surface rendering: {t['materials']}. Ornament: {t['ornament']}. "
    f"Respect 9-slice safe corner insets L{ins['l']} T{ins['t']} R{ins['r']} B{ins['b']} pixels: "
    "keep focal ornament out of stretch zones, maintain continuous edges, keep central field calm and text-free. "
    "Strong silhouette and hierarchy at native UI scale; believable bevels, subtle wear, consistent light direction, "
    "clean anti-aliased edges, no clutter. "+
    ("Transparent background beyond the isolated silhouette, clean straight alpha." if alpha else
     "Fill panel surface; content/text field is empty and unobstructed."))
    negative=("text, typography, letters, digits, words, logos, watermark, signature, screenshot, mockup, "
    "multiple assets, collage, contact sheet, character, hand, landscape, photograph, blurry edges, unreadable runes, "
    "flat monochrome vector, generic repeated icon, clipped frame, cropped corners, perspective distortion, bloom, "
    "chromatic aberration, noisy microtexture, random ornament")
    return {"job_id":sha(f"{name}:{w}x{h}:{seed}:{theme}".encode())[:16],"asset":name,"output":a["file"],
      "width":w,"height":h,"family":kind,"state":states,"theme":theme,"seed":seed,"insets":ins,
      "requires_alpha":alpha,"prompt":prompt,"negative_prompt":negative,
      "output_contract":{"format":"PNG RGBA8","color_space":"sRGB","alpha":"straight","one_asset_only":True,"no_text":True}}

def plan(manifest:Path,out:Path,theme:str,seed:int,engine_contract:bool)->int:
    doc=load_manifest(manifest,engine_contract);out.mkdir(parents=True,exist_ok=True)
    jobs=[make_job(a,theme,seed) for a in doc["assets"]]
    manifest_sha=sha(manifest.read_bytes())
    payload={"tool":"HUD Artisan","version":VERSION,"theme":theme,"seed":seed,
      "source_manifest_sha256":manifest_sha,"job_count":len(jobs),"jobs":jobs}
    (out/"jobs.json").write_text(json.dumps(payload,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    pd=out/"prompts";pd.mkdir(exist_ok=True)
    for j in jobs:
        body=(f"# {j['asset']}\n\nJob: {j['job_id']}\nOutput: {j['output']}\nTarget: {j['width']}x{j['height']} RGBA8 PNG\n"
        f"Family: {j['family']}\nInsets: {j['insets']}\n\n## Prompt\n{j['prompt']}\n\n## Negative prompt\n{j['negative_prompt']}\n")
        (pd/(slug(j["asset"])+".md")).write_text(body,encoding="utf-8")
    brief=f"""# HUD Artisan agent brief

{len(jobs)} independent jobs were compiled from the authoritative manifest.

- Generate exactly one asset per job. Never output a sheet, collage or screenshot.
- Preserve each exact relative output path, native dimensions and straight alpha.
- Respect the declared nine-slice insets: corners retain ornament; stretch zones stay calm and continuous.
- Sibling states must be visibly distinct but use one consistent component language.
- Icons need semantically recognisable, distinct silhouettes at 32–64 px; no generic placeholder rune.
- Panels need material layers, controlled bevel lighting, believable occlusion and restrained wear.
- Keep text out of images; game text is rendered by the engine.
- Clean-room fantasy art only. No extracted game assets, game screenshots, proprietary logos or copied retail art.
- Deliver outputs under the exact paths in your drop folder and include provenance.json with provider/model,
  revision, seed, prompt hash, post-processing and reference-source/license details.
- Structural PASS is not art approval. Inspect at native size and run actual game screenshot QA.

Run:
  python tools/hud_artisan/generate.py ingest --manifest MANIFEST --drop DROP --out PACK
  python tools/hud_artisan/generate.py validate --manifest MANIFEST --pack PACK --pixels
  python tools/hud_artisan/generate.py contact-sheet --manifest MANIFEST --pack PACK --out contact-sheet.png

Theme={theme}; seed={seed}; source manifest SHA-256={manifest_sha}
"""
    (out/"AGENT_BRIEF.md").write_text(brief,encoding="utf-8")
    print(f"HUD_ARTISAN_PLAN_OK jobs={len(jobs)} out={out}");return 0

def png_info(path:Path)->tuple[int,int,int,int]:
    data=path.read_bytes()
    if not data.startswith(PNG_SIGNATURE): raise ValueError("bad PNG signature")
    pos=8;w=h=depth=ctype=None;idat=0;ended=False;ihdr=False
    while pos<len(data):
        if pos+12>len(data): raise ValueError("truncated PNG chunk header")
        n=struct.unpack(">I",data[pos:pos+4])[0];tag=data[pos+4:pos+8];end=pos+12+n
        if end>len(data): raise ValueError(f"truncated {tag!r} chunk")
        payload=data[pos+8:pos+8+n];crc=struct.unpack(">I",data[pos+8+n:end])[0]
        if zlib.crc32(tag+payload)&0xffffffff!=crc: raise ValueError(f"CRC mismatch in {tag!r}")
        if tag==b"IHDR":
            if n!=13 or ihdr: raise ValueError("invalid/duplicate IHDR")
            ihdr=True;w,h,depth,ctype,comp,filt,interlace=struct.unpack(">IIBBBBB",payload)
            if not w or not h or w*h>MAX_PIXELS: raise ValueError("unsafe PNG dimensions")
            if comp or filt or interlace not in (0,1): raise ValueError("unsupported PNG encoding")
        elif tag==b"IDAT": idat+=n
        elif tag==b"IEND":
            if n: raise ValueError("non-empty IEND")
            ended=True
            if end!=len(data): raise ValueError("trailing bytes after IEND")
            break
        pos=end
    if not ihdr or not ended or not idat: raise ValueError("incomplete PNG")
    return int(w),int(h),int(depth),int(ctype)

def pil_modules():
    try:
        from PIL import Image, ImageDraw, ImageStat
        return Image,ImageDraw,ImageStat
    except ImportError: return None

def image_metrics(path:Path)->dict:
    mods=pil_modules()
    if not mods: raise RuntimeError("pixel QA requires Pillow; install tools/hud_artisan/requirements.txt")
    Image,_,ImageStat=mods
    with Image.open(path) as src:
        src.load();rgba=src.convert("RGBA");alpha=rgba.getchannel("A");hist=alpha.histogram()
        trans=sum(hist[:255])/(rgba.width*rgba.height);opaque=hist[255]/(rgba.width*rgba.height)
        thumb=rgba.convert("RGB").resize((min(64,rgba.width),min(64,rgba.height)))
        colors=len(set(thumb.getdata()));variance=sum(ImageStat.Stat(thumb).var)/3
        box=alpha.getbbox()
        return {"source_mode":src.mode,"transparent_fraction":round(trans,5),"opaque_fraction":round(opaque,5),
          "alpha_bbox":list(box) if box else None,"thumbnail_unique_colors":colors,
          "thumbnail_rgb_variance":round(variance,3)}

def validate(manifest:Path,pack:Path,pixels:bool,strict_insets:bool)->int:
    doc=load_manifest(manifest);fail=[];warn=[];records=[];hashes=defaultdict(list)
    if not (pack/"manifest.json").is_file(): fail.append("pack manifest.json missing")
    elif read_json(pack/"manifest.json")!=doc: fail.append("pack manifest differs from source manifest")
    by_file={a["file"]:a for a in doc["assets"]}
    for a in doc["assets"]:
        name=a["name"];rel=safe_rel(a["file"]);f=pack/rel
        if not f.is_file(): fail.append(f"{name}: missing {rel}");continue
        try:
            w,h,depth,ctype=png_info(f)
            if (w,h)!=(a["w"],a["h"]): fail.append(f"{name}: {w}x{h} != {a['w']}x{a['h']}")
            if depth!=8 or ctype!=6: fail.append(f"{name}: expected RGBA8; got depth={depth} color={ctype}")
            digest=sha(f.read_bytes());hashes[digest].append(name)
            rec={"name":name,"file":str(rel),"sha256":digest,"bytes":f.stat().st_size}
            if pixels and (w,h)==(a["w"],a["h"]) and depth==8 and ctype==6:
                m=image_metrics(f);rec["pixels"]=m
                if not m["alpha_bbox"]: fail.append(f"{name}: fully transparent")
                if m["thumbnail_unique_colors"]<12: fail.append(f"{name}: flat image; only {m['thumbnail_unique_colors']} sampled colours")
                if m["thumbnail_rgb_variance"]<2: fail.append(f"{name}: near-uniform pixel variance")
                if family(name) in ("icon","cursor") and m["transparent_fraction"]<.005: warn.append(f"{name}: almost no transparent pixels")
                if family(name) in ("panel","menu-panel","portrait-frame","hud-strip") and m["thumbnail_unique_colors"]<40: warn.append(f"{name}: low material colour diversity")
            records.append(rec)
            ins=a["insets"];l,t,r,b=(float(ins[k]) for k in ("l","t","r","b"))
            if l+r>=w or t+b>=h:
                msg=f"{name}: invalid 9-slice centre (L+R={l+r}/{w}, T+B={t+b}/{h})"
                (fail if strict_insets else warn).append(msg)
        except Exception as e: fail.append(f"{name}: {e}")
    for digest,names in hashes.items():
        if len(names)>1: fail.append("byte-identical assets: "+", ".join(names))
    atlas=doc["atlas"]["commandbuttons"];atlas_file=atlas.get("file")
    if not isinstance(atlas_file,str) or atlas_file not in by_file:
        fail.append("atlas.commandbuttons.file must match an assets[].file")
    else:
        aw,ah=by_file[atlas_file]["w"],by_file[atlas_file]["h"];occupied=[]
        for e in atlas["entries"]:
            try:
                x,y,w,h=(float(e[k]) for k in ("x","y","w","h"))
                if x<0 or y<0 or w<=0 or h<=0 or x+w>aw or y+h>ah: fail.append(f"atlas {e.get('id')}: outside {aw}x{ah}")
                for ox,oy,ow,oh,oid in occupied:
                    if x<ox+ow and ox<x+w and y<oy+oh and oy<y+h: fail.append(f"atlas overlap: {e.get('id')} / {oid}")
                occupied.append((x,y,w,h,e.get("id","?")))
            except Exception: fail.append(f"invalid atlas entry: {e!r}")
    for font in doc["font_files"]:
        if not isinstance(font,str) or Path(font).name!=font: fail.append(f"unsafe font filename: {font!r}")
        elif not (pack/"fonts"/font).is_file(): fail.append(f"missing font dependency fonts/{font}")
    report={"tool":"HUD Artisan","version":VERSION,"manifest_sha256":sha(manifest.read_bytes()),
      "asset_count":len(doc["assets"]),"checked_count":len(records),"pixel_qa":pixels,
      "failures":fail,"warnings":warn,"assets":records}
    (pack/"reports").mkdir(exist_ok=True)
    (pack/"reports"/"validation.json").write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    for x in warn: print("HUD_ARTISAN_WARN "+x)
    for x in fail: print("HUD_ARTISAN_FAIL "+x)
    print(f"HUD_ARTISAN_VALIDATE {'PASS' if not fail else 'FAIL'} assets={len(records)}/{len(doc['assets'])} warnings={len(warn)} failures={len(fail)}")
    return int(bool(fail))

def ingest(manifest:Path,drop:Path,out:Path,fit:str,allow_opaque:bool)->int:
    doc=load_manifest(manifest);mods=pil_modules()
    if not mods: raise RuntimeError("ingestion requires Pillow; install tools/hud_artisan/requirements.txt")
    Image,_,_=mods;out.mkdir(parents=True,exist_ok=True);shutil.copy2(manifest,out/"manifest.json")
    (out/"fonts").mkdir(exist_ok=True);done=[];errors=[];inputs=[]
    for a in doc["assets"]:
        rel=safe_rel(a["file"]);src=drop/rel
        alt=drop/(slug(a["name"])+".png")
        if not src.is_file() and alt.is_file():src=alt
        if not src.is_file():errors.append(f"{a['name']}: missing input {rel}");continue
        try:
            with Image.open(src) as im:
                im.load();original=im.size;im=im.convert("RGBA");target=(a["w"],a["h"])
                if im.size!=target:
                    if fit=="strict": raise ValueError(f"input {im.size} != target {target}; choose fit policy explicitly")
                    if fit=="contain":
                        im.thumbnail(target,Image.Resampling.LANCZOS);canvas=Image.new("RGBA",target,(0,0,0,0))
                        canvas.alpha_composite(im,((target[0]-im.width)//2,(target[1]-im.height)//2));im=canvas
                    elif fit=="crop":
                        scale=max(target[0]/im.width,target[1]/im.height);size=(round(im.width*scale),round(im.height*scale))
                        im=im.resize(size,Image.Resampling.LANCZOS);x=(size[0]-target[0])//2;y=(size[1]-target[1])//2
                        im=im.crop((x,y,x+target[0],y+target[1]))
                    elif fit=="stretch":im=im.resize(target,Image.Resampling.LANCZOS)
                if family(a["name"]) in ("icon","cursor") and not allow_opaque and im.getchannel("A").getextrema()==(255,255):
                    raise ValueError("icon/cursor is opaque; preserve transparent background or pass --allow-opaque")
                dst=out/rel;dst.parent.mkdir(parents=True,exist_ok=True);im.save(dst,"PNG",optimize=True,compress_level=9)
                inputs.append({"asset":a["name"],"input":str(src),"input_sha256":sha(src.read_bytes()),
                   "output_sha256":sha(dst.read_bytes()),"original_size":original,"final_size":im.size});done.append(a["name"])
        except Exception as e:errors.append(f"{a['name']}: {e}")
    for font in doc["font_files"]:
        if not isinstance(font,str) or Path(font).name!=font:errors.append(f"unsafe font filename {font!r}");continue
        src=drop/"fonts"/font
        if src.is_file():shutil.copy2(src,out/"fonts"/font)
        else:errors.append(f"missing font dependency fonts/{font}")
    (out/"reports").mkdir(exist_ok=True)
    (out/"reports"/"ingestion.json").write_text(json.dumps({"tool":"HUD Artisan","version":VERSION,
      "manifest_sha256":sha(manifest.read_bytes()),"ingested":done,"inputs":inputs,"errors":errors,"fit_policy":fit},
      ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    for e in errors:print("HUD_ARTISAN_FAIL "+e)
    print(f"HUD_ARTISAN_INGEST {'PASS' if not errors else 'FAIL'} imported={len(done)}/{len(doc['assets'])} errors={len(errors)}")
    return int(bool(errors))

def contact_sheet(manifest:Path,pack:Path,out:Path,cols:int,cell:int)->int:
    doc=load_manifest(manifest);mods=pil_modules()
    if not mods:raise RuntimeError("contact sheet requires Pillow")
    Image,ImageDraw,_=mods;label=34;rows=math.ceil(len(doc["assets"])/cols)
    canvas=Image.new("RGB",(cols*cell,rows*(cell+label)),(28,27,25));draw=ImageDraw.Draw(canvas)
    for i,a in enumerate(doc["assets"]):
        x=(i%cols)*cell;y=(i//cols)*(cell+label);src=pack/safe_rel(a["file"])
        if not src.is_file():draw.text((x+8,y+8),"MISSING "+a["name"],fill=(255,80,70));continue
        with Image.open(src) as im:
            im=im.convert("RGBA");im.thumbnail((cell-20,cell-20),Image.Resampling.LANCZOS)
            tile=Image.new("RGBA",(cell-12,cell-12),(52,51,48,255))
            tile.alpha_composite(im,((tile.width-im.width)//2,(tile.height-im.height)//2));canvas.paste(tile.convert("RGB"),(x+6,y+6))
        draw.text((x+8,y+cell+2),a["name"][:40],fill=(236,224,198))
    out.parent.mkdir(parents=True,exist_ok=True);canvas.save(out)
    print(f"HUD_ARTISAN_CONTACT_SHEET {out} {canvas.width}x{canvas.height}");return 0

def main(argv=None):
    p=argparse.ArgumentParser(description="HUD Artisan — provider-neutral multi-agent art production")
    s=p.add_subparsers(dest="cmd",required=True)
    q=s.add_parser("profile",help="write verified engine contract reference, not a fabricated manifest")
    q.add_argument("--out",type=Path,required=True)
    q=s.add_parser("plan",help="compile per-asset generation jobs and prompts")
    q.add_argument("--manifest",type=Path,required=True);q.add_argument("--out",type=Path,required=True)
    q.add_argument("--theme",choices=tuple(THEMES),default="classic-modern");q.add_argument("--seed",type=int,default=20261118)
    q.add_argument("--engine-contract",action="store_true")
    q=s.add_parser("ingest",help="normalize agent PNG drop into a pack")
    q.add_argument("--manifest",type=Path,required=True);q.add_argument("--drop",type=Path,required=True);q.add_argument("--out",type=Path,required=True)
    q.add_argument("--fit",choices=("strict","contain","crop","stretch"),default="strict");q.add_argument("--allow-opaque",action="store_true")
    q=s.add_parser("validate",help="verify PNGs, atlas, fonts and optional pixel quality heuristics")
    q.add_argument("--manifest",type=Path,required=True);q.add_argument("--pack",type=Path,required=True)
    q.add_argument("--pixels",action="store_true");q.add_argument("--strict-insets",action="store_true")
    q=s.add_parser("contact-sheet",help="build labeled visual review sheet")
    q.add_argument("--manifest",type=Path,required=True);q.add_argument("--pack",type=Path,required=True);q.add_argument("--out",type=Path,required=True)
    q.add_argument("--cols",type=int,default=5);q.add_argument("--cell",type=int,default=240)
    a=p.parse_args(argv)
    try:
        if a.cmd=="profile":
            profile={"profile":"wc3rust-hud-engine-contract","contract_only":True,"design_height":768,
             "reference_resolution":{"landscape":[2414,1080],"portrait":[1080,2414]},
             "safe_area_1080p":{"landscape":[48,80,48,48],"portrait":[24,96,24,64]},
             "command_card":{"columns":4,"rows":3,"cell":[64,64],"gap":4},
             "control_groups":{"count":10,"cell":[44,44]},
             "commandbuttons_atlas":{"size":[2048,2048],"entries":20},
             "font_files_required":5,"required_unique_assets":ENGINE_ROLES,
             "note":"Contract reference only. Unknown insets, complete filenames, font binaries and source art are intentionally not fabricated."}
            a.out.parent.mkdir(parents=True,exist_ok=True);a.out.write_text(json.dumps(profile,indent=2)+"\n",encoding="utf-8")
            print(f"HUD_ARTISAN_PROFILE_OK contract_only=true roles={len(ENGINE_ROLES)}");return 0
        if a.cmd=="plan":return plan(a.manifest,a.out,a.theme,a.seed,a.engine_contract)
        if a.cmd=="ingest":return ingest(a.manifest,a.drop,a.out,a.fit,a.allow_opaque)
        if a.cmd=="validate":return validate(a.manifest,a.pack,a.pixels,a.strict_insets)
        if a.cmd=="contact-sheet":return contact_sheet(a.manifest,a.pack,a.out,a.cols,a.cell)
    except (ValueError,RuntimeError,OSError) as e:
        print(f"HUD_ARTISAN_ERROR {e}",file=sys.stderr);return 2
    return 2
if __name__=="__main__":raise SystemExit(main())
