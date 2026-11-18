#!/usr/bin/env python3
"""HUD Artisan 1.0: manifest-driven art production, ingestion and quality gates.

The tool is provider-neutral: an image model or agent consumes jobs.json and writes
one PNG per job into a drop directory. Python stdlib handles planning and PNG integrity;
Pillow provides decode, normalization and pixel-level QA.
"""
from __future__ import annotations
import argparse, concurrent.futures, datetime, hashlib, json, math, os, re, shutil, struct, subprocess, sys, tempfile, zlib
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
def safe_join(root:Path,relative:str|Path)->Path:
    base=root.resolve()
    rel=safe_rel(str(relative))
    target=(base/rel).resolve()
    if not target.is_relative_to(base):
        raise ValueError(f"path escapes its root through a symlink: {relative}")
    return target

def read_json(path:Path)->dict[str,Any]:
    try: obj=json.loads(path.read_text(encoding="utf-8"))
    except (OSError,json.JSONDecodeError) as e: raise ValueError(f"cannot read {path}: {e}") from e
    if not isinstance(obj,dict): raise ValueError(f"{path}: JSON root must be an object")
    return obj

def validate_engine_contract(doc:dict[str,Any])->None:
    """Fail closed when a claimed production manifest drifts from the Rust HUD contract."""
    expected_resolution={"landscape":[2414,1080],"portrait":[1080,2414]}
    expected_safe={"landscape":[48,80,48,48],"portrait":[24,96,24,64]}
    if doc.get("design_height")!=768:
        raise ValueError(f"engine contract design_height must be 768, found {doc.get('design_height')}")
    if doc.get("reference_resolution")!=expected_resolution:
        raise ValueError(f"engine contract reference_resolution mismatch: expected {expected_resolution}")
    if doc.get("safe_area_1080p")!=expected_safe:
        raise ValueError(f"engine contract safe areas mismatch: expected {expected_safe}")
    card=doc.get("command_card") or {}
    if card.get("columns")!=4 or card.get("rows")!=3 or card.get("cell")!=[64,64] or card.get("gap")!=4:
        raise ValueError("engine contract command_card must be 4x3, 64x64 cells, gap 4")
    groups=doc.get("control_groups") or {}
    if groups.get("count")!=10 or groups.get("cell")!=[44,44]:
        raise ValueError("engine contract control_groups must be ten 44x44 cells")
    assets=doc.get("assets") or []
    if len(assets)!=92:
        raise ValueError(f"engine contract requires 92 assets, found {len(assets)}")
    by_file={a.get("file"):a for a in assets}
    atlas=(doc.get("atlas") or {}).get("commandbuttons") or {}
    atlas_file=atlas.get("file")
    atlas_asset=by_file.get(atlas_file)
    if not atlas_asset:
        raise ValueError("engine contract commandbuttons atlas file must be a declared asset")
    if (atlas_asset.get("w"),atlas_asset.get("h"))!=(2048,2048):
        raise ValueError(f"engine contract commandbuttons atlas must be 2048x2048, found {atlas_asset.get('w')}x{atlas_asset.get('h')}")
    entries=atlas.get("entries")
    if not isinstance(entries,list) or len(entries)!=20:
        raise ValueError(f"engine contract commandbuttons atlas requires 20 entries, found {len(entries) if isinstance(entries,list) else 0}")
    names={a.get("name") for a in assets}
    absent=sorted(set(ENGINE_ROLES)-names)
    if absent: raise ValueError("required engine roles missing: "+", ".join(absent))

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
    for font in doc["font_files"]:
        if not isinstance(font,str) or not font or Path(font).name!=font or font in (".",".."):
            raise ValueError(f"font_files entries must be safe basenames, got {font!r}")
    if engine_contract:
        validate_engine_contract(doc)
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

def semantic_brief(name:str,kind:str)->str:
    n=name.lower()
    catalog=[
      (("gold","coin"),"a minted gold coin with milled edge and embossed abstract crest; no lettering"),
      (("lumber","wood"),"a bundle of cut timber logs with visible end grain and a binding"),
      (("food","meat","supply"),"a roast, grain or ration emblem with a distinct food silhouette"),
      (("attack","sword","damage"),"a forged blade with recognisable guard, grip and cutting edge"),
      (("armor","armour","shield","defense"),"a heraldic shield with a raised boss and layered metal rim"),
      (("health","life","heal","hp"),"a vitality emblem with a strong heart or healing motif"),
      (("mana","magic","rune"),"a faceted arcane crystal or contained magical-energy motif"),
      (("move","rally"),"a directional movement arrow or rally pennant with strong forward silhouette"),
      (("cancel","close","stop"),"a decisive crossed-blade or barred cancellation motif, not a text glyph"),
      (("build","repair"),"a builder hammer, tool or scaffold motif"),
      (("research","upgrade"),"an alchemical research apparatus or crafted ascending emblem"),
      (("train","worker","peasant"),"a worker/tool insignia with a clear tool head and handle"),
      (("patrol",),"a circular route or guard marker with one directional notch"),
      (("fire","flame"),"a sculpted flame with bright core and dark outer silhouette"),
      (("frost","ice"),"a six-point frost crystal with crisp separated arms"),
      (("lightning","thunder"),"a jagged lightning bolt with a clear compact contour"),
      (("summon",),"a summoning sigil with central focal gem and radiating structure"),
      (("stealth","invis"),"a veiled or hooded silhouette suggesting concealment"),
      (("teleport",),"a portal ring with central aperture and one energy arc"),
      (("potion",),"a stoppered potion flask with distinct neck and liquid level"),
      (("scroll",),"a rolled parchment scroll with visibly curled ends"),
      (("boots",),"a crafted boot silhouette with reinforced sole and leather cuff"),
      (("ring",),"a metal ring in three-quarter view with one inset gem"),
      (("orb",),"a suspended faceted orb with controlled internal glow"),
      (("bow",),"a recurved bow with visible string, grip and tapered limbs"),
      (("axe",),"a forged axe head and haft with readable cutting edge"),
      (("staff",),"a caster staff with distinct head ornament and tapered shaft"),
      (("tower",),"a defensive tower with strong base and crenellated crown"),
      (("castle",),"a fortified keep with distinct towers and gate"),
      (("cavalry",),"a mounted-warrior or horse-head emblem distinct from infantry"),
      (("infantry",),"an infantry helmet and weapon silhouette distinct from mage and worker"),
      (("mage",),"a caster hood and arcane staff silhouette distinct from worker"),
      (("demon",),"a horned demonic profile with one compact readable contour"),
      (("undead",),"a bone, skull or spectral emblem with clear negative space"),
      (("nature",),"an organic leaf, antler or life motif with layered planes"),
    ]
    if kind=="icon":
        for terms,description in catalog:
            if any(term in n for term in terms):
                return description
        semantic=n.replace("_"," ").replace("icon ","").replace("ability ","").replace("item ","")
        if "spell" in n:
            return f"an original spell emblem for {semantic}, with an effect-specific silhouette and no generic placeholder rune"
        return f"a distinctive emblem representing {semantic}; never substitute a generic rune"
    if kind=="portrait-frame":
        return "a portrait surround only, with an empty aperture reserved for the separately rendered face"
    if kind=="minimap-frame":
        return "a map viewport surround with a completely unobstructed central map window"
    if kind=="interactive-control":
        return "a tactile button or slot with a legible interactive surface and precise perimeter"
    if kind=="hud-strip":
        return "a horizontal information plate with stretch-safe centre and distinctive end caps"
    if kind=="menu-panel":
        return "a large menu/dialog panel with a calm content field and layered inner/outer borders"
    if kind=="panel":
        return "a reusable UI panel with clear perimeter, dark central field and nine-slice-safe construction"
    return f"a functional fantasy RTS interface element representing {n.replace('_',' ')}"

def state_brief(state_text:str)->str:
    s=state_text.lower()
    rules=[]
    if "hover" in s: rules.append("hover state: subtle edge illumination and slight value lift; no broad glow")
    if "pressed" in s: rules.append("pressed state: tactile depressed face, darkened inner bevel and compressed highlight")
    if "selected" in s: rules.append("selected state: distinct restrained selection rim and unmistakable focus")
    if "disabled" in s: rules.append("disabled state: lower saturation and luminance while preserving silhouette")
    if "autocast" in s: rules.append("autocast state: precise small circular enchantment rim; centre symbol remains readable")
    if "cooldown" in s: rules.append("cooldown state: preserve clear space for the engine's radial dark cooldown mask; no numbers")
    if "active" in s and "inactive" not in s: rules.append("active state: stronger focal contrast and brighter bevel")
    if "inactive" in s: rules.append("inactive state: neutral quiet bevel with no selection highlight")
    if "warn" in s: rules.append("warning state: restrained warm danger accent; never a broad red wash")
    if "empty" in s: rules.append("empty state: unoccupied dark inset, with no invented item or ability")
    return "; ".join(rules) if rules else "normal state: balanced material values and neutral illumination"

def make_job(a:dict,theme:str,seed:int)->dict:
    name,w,h=a["name"],a["w"],a["h"];kind=family(name);t=THEMES[theme];ins=a["insets"]
    states=", ".join(a.get("states") or []) or "normal/default state"
    alpha=kind in ("icon","cursor")
    prompt=(f"Create ONE production game UI asset named {name}; exact canvas {w}x{h}, ratio {w/h:.5f}. "
    f"Family {kind}; state {states}. Semantic subject: {semantic_brief(name,kind)}. "
    f"State rendering instruction: {state_brief(states)}. "
    f"Palette: {t['palette']}. Materials: {material(name,kind)}. "
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
    prompt_sha256=sha(prompt.encode("utf-8"))
    job_id=sha(f"{name}:{w}x{h}:{seed}:{theme}:{prompt_sha256}".encode())[:16]
    return {"job_id":job_id,"prompt_sha256":prompt_sha256,"asset":name,"output":a["file"],
      "width":w,"height":h,"family":kind,"state":states,"theme":theme,"seed":seed,"insets":ins,
      "requires_alpha":alpha,"prompt":prompt,"negative_prompt":negative,
      "output_contract":{"format":"PNG RGBA8","color_space":"sRGB","alpha":"straight","one_asset_only":True,"no_text":True}}

def plan(manifest:Path,out:Path,theme:str,seed:int,engine_contract:bool)->int:
    doc=load_manifest(manifest,engine_contract);out.mkdir(parents=True,exist_ok=True)
    atlas=doc["atlas"]["commandbuttons"]
    atlas_file=atlas.get("file")
    by_name={a["name"]:a for a in doc["assets"]}
    by_file={a["file"]:a for a in doc["assets"]}
    if atlas_file not in by_file:
        raise ValueError("atlas.commandbuttons.file must resolve to an assets[].file entry")
    atlas_asset=by_file[atlas_file]
    jobs=[];cells=[]
    # The atlas image itself is assembled from its independently generated cells.
    # Never ask an image model to produce a single 2048x2048 sprite sheet.
    for a in doc["assets"]:
        if a["file"] != atlas_file:
            jobs.append(make_job(a,theme,seed))
    occupied=[];cell_ids=set()
    for entry in atlas["entries"]:
        ident=str(entry.get("id","")).strip()
        if not ident or ident in cell_ids: raise ValueError(f"atlas cell id is empty or duplicated: {ident!r}")
        cell_ids.add(ident)
        x,y,w,h=(int(round(float(entry[k]))) for k in ("x","y","w","h"))
        if x<0 or y<0 or w<1 or h<1 or x+w>atlas_asset["w"] or y+h>atlas_asset["h"]:
            raise ValueError(f"atlas cell {ident}: rectangle {(x,y,w,h)} outside {atlas_asset['w']}x{atlas_asset['h']}")
        for ox,oy,ow,oh,other in occupied:
            if x<ox+ow and ox<x+w and y<oy+oh and oy<y+h:
                raise ValueError(f"atlas cells overlap: {ident} and {other}")
        occupied.append((x,y,w,h,ident))
        source_asset=entry.get("source_asset") or entry.get("asset")
        source_file=entry.get("source_file")
        if source_asset in by_name: source_file=by_name[source_asset]["file"]
        elif source_asset in by_file: source_file=source_asset
        elif source_asset:
            raise ValueError(f"atlas cell {ident}: unknown source_asset {source_asset!r}")
        elif ident in by_name and by_name[ident]["file"] != atlas_file:
            source_asset=ident;source_file=by_name[ident]["file"]
        if not source_file or source_file not in by_file:
            file=source_file or f"atlas_sources/{slug(ident)}.png"
            file=str(safe_rel(file))
            pseudo={"name":f"command_icon_{slug(ident)}","file":file,"w":w,"h":h,
                    "insets":{"l":0,"t":0,"r":0,"b":0},"states":[]}
            jobs.append(make_job(pseudo,theme,seed))
            source_file=file
        cells.append({"id":ident,"x":x,"y":y,"w":w,"h":h,"source_asset":source_asset,"source_file":source_file})
    payload={"tool":"HUD Artisan","schema_version":1,"version":VERSION,"theme":theme,"seed":seed,
      "engine_contract":engine_contract,"source_manifest_sha256":sha(manifest.read_bytes()),"job_count":len(jobs),"jobs":jobs,
      "atlas":{"file":atlas_file,"width":atlas_asset["w"],"height":atlas_asset["h"],"cells":cells}}
    (out/"jobs.json").write_text(json.dumps(payload,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    pd=out/"prompts"
    if pd.exists(): shutil.rmtree(pd)
    pd.mkdir(parents=True,exist_ok=True)
    prompt_names=set()
    for j in jobs:
        body=(f"# {j['asset']}\n\nJob: {j['job_id']}\nOutput: {j['output']}\nTarget: {j['width']}x{j['height']} RGBA8 PNG\n"
        f"Family: {j['family']}\nInsets: {j['insets']}\n\n## Prompt\n{j['prompt']}\n\n## Negative prompt\n{j['negative_prompt']}\n")
        prompt_name=slug(j["asset"])+"-"+j["job_id"][:8]+".md"
        if prompt_name in prompt_names: raise ValueError(f"prompt filename collision: {prompt_name}")
        prompt_names.add(prompt_name)
        (pd/prompt_name).write_text(body,encoding="utf-8")
    brief=f"""# HUD Artisan agent brief

{len(jobs)} independent image jobs were compiled. The command-buttons atlas is assembled later by
the tool from atlas cell jobs and must never be generated as a single model-created collage.

- Generate exactly one asset per job; never output a sheet, collage or screenshot.
- Read STYLE_BIBLE.md and apply its material system, icon semantics, UI-state grammar and rejection rules.
- Preserve exact relative output paths, native dimensions and straight alpha.
- Respect declared nine-slice insets; corners retain ornament and stretch zones remain calm.
- Sibling states must be visibly distinct but coherent. Icons need distinct semantic silhouettes.
- Panels need layered materials, controlled bevel lighting, believable occlusion and restrained wear.
- Keep text out of images; the engine renders text.
- Clean-room fantasy art only. No extracted game assets, screenshots, logos or copied retail artwork.
- Deliver PNGs under the declared paths and provenance.json with provider/model/revision, seed,
  prompt hash, references, license and post-processing.
- Structural PASS is not art approval; inspect at native size and perform in-engine screenshot QA.

The plan also contains atlas cell coordinates and source paths. The ingest step composites these
cells into the declared atlas asset file at exact coordinates.

Run:
  python tools/hud_artisan/generate.py ingest --manifest MANIFEST --jobs JOBS_JSON --drop DROP --out PACK
  python tools/hud_artisan/generate.py validate --manifest MANIFEST --pack PACK --pixels
  python tools/hud_artisan/generate.py contact-sheet --manifest MANIFEST --pack PACK --out contact-sheet.png

Theme={theme}; seed={seed}; source manifest SHA-256={sha(manifest.read_bytes())}
"""
    (out/"AGENT_BRIEF.md").write_text(brief,encoding="utf-8")
    print(f"HUD_ARTISAN_PLAN_OK jobs={len(jobs)} atlas_cells={len(cells)} out={out}");return 0

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

def validate(manifest:Path,pack:Path,pixels:bool,strict_insets:bool,engine_contract:bool=False)->int:
    doc=load_manifest(manifest,engine_contract);fail=[];warn=[];records=[];hashes=defaultdict(list)
    if not (pack/"manifest.json").is_file(): fail.append("pack manifest.json missing")
    elif read_json(pack/"manifest.json")!=doc: fail.append("pack manifest differs from source manifest")
    by_file={a["file"]:a for a in doc["assets"]}
    for a in doc["assets"]:
        name=a["name"];rel=safe_rel(a["file"]);f=safe_join(pack,rel)
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
      "asset_count":len(doc["assets"]),"checked_count":len(records),"pixel_qa":pixels,"engine_contract":engine_contract,
      "failures":fail,"warnings":warn,"assets":records}
    (pack/"reports").mkdir(exist_ok=True)
    (pack/"reports"/"validation.json").write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    for x in warn: print("HUD_ARTISAN_WARN "+x)
    for x in fail: print("HUD_ARTISAN_FAIL "+x)
    print(f"HUD_ARTISAN_VALIDATE {'PASS' if not fail else 'FAIL'} assets={len(records)}/{len(doc['assets'])} warnings={len(warn)} failures={len(fail)}")
    return int(bool(fail))

def ingest(manifest:Path,jobs_path:Path,drop:Path,out:Path,fit:str,allow_opaque:bool)->int:
    plan_doc=read_json(jobs_path);doc=load_manifest(manifest,bool(plan_doc.get("engine_contract")));mods=pil_modules()
    if not mods: raise RuntimeError("ingestion requires Pillow; install tools/hud_artisan/requirements.txt")
    if plan_doc.get("tool")!="HUD Artisan" or plan_doc.get("schema_version")!=1:
        raise ValueError("unsupported jobs.json tool/schema")
    if plan_doc.get("source_manifest_sha256") != sha(manifest.read_bytes()):
        raise ValueError("jobs.json was compiled from a different manifest revision")
    if not isinstance(plan_doc.get("engine_contract"),bool):
        raise ValueError("jobs.json is missing its engine_contract flag")
    planned_jobs=plan_doc.get("jobs")
    if not isinstance(planned_jobs,list) or not planned_jobs:
        raise ValueError("jobs.json has no jobs[]")
    if any(job.get("prompt_sha256")!=sha(job.get("prompt","").encode("utf-8")) for job in planned_jobs):
        raise ValueError("jobs.json contains a prompt whose prompt_sha256 does not match")
    out=out.resolve()
    if out.exists() and any(out.iterdir()):
        raise ValueError(f"staging output must be empty; refusing to overwrite existing files: {out}")
    out.mkdir(parents=True,exist_ok=True)
    Image,_,_=mods
    shutil.copy2(manifest,out/"manifest.json")
    (out/"fonts").mkdir(exist_ok=True);done=[];errors=[];inputs=[]
    # Verify chain-of-custody before importing any generated output.
    provenance_path=drop/"provenance.json"
    provenance=None
    if not provenance_path.is_file():
        errors.append("missing drop/provenance.json")
    else:
        try:
            provenance=read_json(provenance_path)
            if provenance.get("jobs_sha256") != sha(jobs_path.read_bytes()):
                raise ValueError("provenance jobs_sha256 does not match the supplied jobs.json")
            rows=provenance.get("jobs")
            if not isinstance(rows,list): raise ValueError("provenance.jobs must be an array")
            by_id={row.get("job_id"):row for row in rows if isinstance(row,dict)}
            expected={job["job_id"]:job for job in plan_doc.get("jobs",[])}
            if set(by_id)!=set(expected):
                raise ValueError(f"provenance job set mismatch: expected {len(expected)}, received {len(by_id)}")
            for jid,job in expected.items():
                row=by_id[jid]
                for key in ("provider","model","generated_at","license","prompt_sha256","output_sha256"):
                    if not isinstance(row.get(key),str) or not row[key].strip():
                        raise ValueError(f"{jid}: provenance field {key} is required")
                if row["prompt_sha256"] != sha(job["prompt"].encode("utf-8")):
                    raise ValueError(f"{jid}: prompt hash mismatch")
                artifact=safe_join(drop,job["output"])
                if not artifact.is_file(): raise ValueError(f"{jid}: missing generated output {job['output']}")
                if row["output_sha256"] != sha(artifact.read_bytes()):
                    raise ValueError(f"{jid}: output hash mismatch for {job['output']}")
        except Exception as exc:
            errors.append(f"provenance: {exc}")
    atlas_file=doc["atlas"]["commandbuttons"]["file"]
    for a in doc["assets"]:
        if a["file"]==atlas_file: continue
        rel=safe_rel(a["file"]);src=safe_join(drop,rel)
        if not src.is_file():errors.append(f"{a['name']}: missing input {rel}");continue
        try:
            with Image.open(src) as im:
                im.load();original=im.size;im=im.convert("RGBA");target=(a["w"],a["h"])
                if im.size!=target:
                    if fit=="strict":raise ValueError(f"input {im.size} != target {target}; choose fit policy explicitly")
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
                dst=safe_join(out,rel);dst.parent.mkdir(parents=True,exist_ok=True);im.save(dst,"PNG",optimize=True,compress_level=9)
                inputs.append({"asset":a["name"],"input":str(src),"input_sha256":sha(src.read_bytes()),
                  "output_sha256":sha(dst.read_bytes()),"original_size":original,"final_size":im.size});done.append(a["name"])
        except Exception as exc:errors.append(f"{a['name']}: {exc}")
    # Atlas is a deterministic composition of cell outputs, never a model-created contact sheet.
    atlas=plan_doc.get("atlas") or {}
    if atlas.get("file")!=atlas_file:errors.append("jobs.json atlas target differs from manifest")
    else:
        canvas=Image.new("RGBA",(int(atlas.get("width",0)),int(atlas.get("height",0))),(0,0,0,0))
        if canvas.width<=0 or canvas.height<=0:errors.append("jobs.json atlas dimensions invalid")
        else:
            for cell in atlas.get("cells",[]):
                try:
                    rel=safe_rel(cell["source_file"])
                    out_source=safe_join(out,rel);drop_source=safe_join(drop,rel)
                    src=out_source if out_source.is_file() else drop_source
                    if not src.is_file():raise ValueError(f"missing source {rel}")
                    with Image.open(src) as im:
                        im.load();tile=im.convert("RGBA")
                    if tile.size!=(int(cell["w"]),int(cell["h"])):
                        raise ValueError(f"cell source size {tile.size} != {(cell['w'],cell['h'])}")
                    x,y=int(cell["x"]),int(cell["y"])
                    if x<0 or y<0 or x+tile.width>canvas.width or y+tile.height>canvas.height:
                        raise ValueError("cell outside atlas")
                    canvas.alpha_composite(tile,(x,y))
                except Exception as exc:errors.append(f"atlas cell {cell.get('id')}: {exc}")
            dst=safe_join(out,atlas_file);dst.parent.mkdir(parents=True,exist_ok=True)
            canvas.save(dst,"PNG",optimize=True,compress_level=9);done.append("atlas:"+atlas_file)
    for font in doc["font_files"]:
        if not isinstance(font,str) or Path(font).name!=font:errors.append(f"unsafe font filename {font!r}");continue
        src=(drop/"fonts"/font).resolve()
        if not src.is_relative_to(drop.resolve()):errors.append(f"font path escapes drop: {font}");continue
        dst=(out/"fonts"/font).resolve()
        if not dst.is_relative_to(out.resolve()):errors.append(f"font destination escapes staging pack: {font}")
        elif src.is_file():shutil.copy2(src,dst)
        else:errors.append(f"missing font dependency fonts/{font}")
    if provenance is not None:
        shutil.copy2(provenance_path,out/"provenance.json")
    (out/"reports").mkdir(exist_ok=True)
    (out/"reports"/"ingestion.json").write_text(json.dumps({"tool":"HUD Artisan","version":VERSION,
      "manifest_sha256":sha(manifest.read_bytes()),"jobs_sha256":sha(jobs_path.read_bytes()),
      "provenance_sha256":sha(provenance_path.read_bytes()) if provenance_path.is_file() else None,
      "ingested":done,"inputs":inputs,"errors":errors,"fit_policy":fit},ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    for error in errors:print("HUD_ARTISAN_FAIL "+error)
    print(f"HUD_ARTISAN_INGEST {'PASS' if not errors else 'FAIL'} imported={len(done)} errors={len(errors)}")
    return int(bool(errors))

def dispatch(jobs_path:Path,adapter_path:Path,drop:Path,parallel_override:int|None)->int:
    payload=read_json(jobs_path);cfg=read_json(adapter_path)
    if payload.get("tool")!="HUD Artisan" or payload.get("schema_version")!=1:
        raise ValueError("unsupported jobs.json tool/schema")
    if not isinstance(payload.get("engine_contract"),bool):
        raise ValueError("jobs.json is missing its engine_contract flag")
    if not re.fullmatch(r"[a-f0-9]{64}",str(payload.get("source_manifest_sha256",""))):
        raise ValueError("jobs.json has an invalid source manifest SHA-256")
    jobs=payload.get("jobs")
    if not isinstance(jobs,list) or not jobs:raise ValueError("jobs.json has no jobs[]")
    command=cfg.get("command")
    if not isinstance(command,list) or not command or not all(isinstance(x,str) for x in command):
        raise ValueError("adapter.command must be a non-empty argv array; shell commands are not accepted")
    if not any("{job_json}" in x for x in command) or not any("{output_path}" in x for x in command):
        raise ValueError("adapter.command must include {job_json} and {output_path}")
    provider=cfg.get("provider");model=cfg.get("model");license_name=cfg.get("license")
    if not all(isinstance(x,str) and x.strip() for x in (provider,model,license_name)):
        raise ValueError("adapter provider, model and license strings are required for provenance")
    timeout=int(cfg.get("timeout_seconds",1800));retries=int(cfg.get("retries",1))
    parallel=int(parallel_override or cfg.get("max_parallel",1))
    if not 1<=timeout<=86400 or not 0<=retries<=5 or not 1<=parallel<=32:
        raise ValueError("adapter timeout/retries/max_parallel outside safe limits")
    allow=cfg.get("environment",[])
    if not isinstance(allow,list) or any(not isinstance(x,str) for x in allow):
        raise ValueError("adapter.environment must be an array of environment variable names")
    ids=set();paths=set()
    for job in jobs:
        jid=job.get("job_id");rel=safe_rel(job.get("output",""))
        if not isinstance(jid,str) or jid in ids:raise ValueError(f"missing/duplicate job id: {jid!r}")
        prompt=job.get("prompt","")
        if not isinstance(prompt,str) or job.get("prompt_sha256")!=sha(prompt.encode("utf-8")):
            raise ValueError(f"{jid}: prompt hash mismatch in jobs.json")
        width,height=job.get("width"),job.get("height")
        if type(width) is not int or type(height) is not int or width<1 or height<1 or width*height>MAX_PIXELS:
            raise ValueError(f"{jid}: invalid target dimensions {width}x{height}")
        if str(rel) in paths:raise ValueError(f"two jobs target the same output path: {rel}")
        ids.add(jid);paths.add(str(rel))
    work=Path(tempfile.mkdtemp(prefix="hud-artisan-"))
    logs=drop/"reports"/"worker";logs.mkdir(parents=True,exist_ok=True)
    drop.mkdir(parents=True,exist_ok=True)
    work_jobs=work/"jobs";work_jobs.mkdir()
    cwd_value=cfg.get("working_directory",".")
    cwd=Path(cwd_value).expanduser()
    if not cwd.is_absolute():cwd=(adapter_path.resolve().parent/cwd).resolve()
    if not cwd.is_dir():raise ValueError(f"adapter working_directory missing: {cwd}")
    env={"PATH":os.environ.get("PATH",""),"HOME":os.environ.get("HOME","")}
    for key in allow:
        if key in os.environ:env[key]=os.environ[key]
    def run_one(job):
        jid=job["job_id"];out=safe_join(drop,job["output"]);out.parent.mkdir(parents=True,exist_ok=True)
        job_file=work_jobs/(jid+".json")
        job_file.write_text(json.dumps(job,ensure_ascii=False,indent=2),encoding="utf-8")
        replacements={"{job_json}":str(job_file),"{output_path}":str(out),"{job_id}":jid}
        argv=[]
        for part in command:
            for token,value in replacements.items():part=part.replace(token,value)
            argv.append(part)
        history=[];status="failed";error=None
        for attempt in range(retries+1):
            try:
                if out.exists():out.unlink()
                result=subprocess.run(argv,cwd=cwd,env=env,capture_output=True,text=True,timeout=timeout,check=False)
                stdout=result.stdout or "";stderr=result.stderr or ""
                for secret_name in allow:
                    secret=env.get(secret_name)
                    if secret:
                        stdout=stdout.replace(secret,"[REDACTED]")
                        stderr=stderr.replace(secret,"[REDACTED]")
                if len(stdout)>131072: stdout=stdout[:131072]+"\n[stdout truncated at 128 KiB]"
                if len(stderr)>131072: stderr=stderr[:131072]+"\n[stderr truncated at 128 KiB]"
                history.append(f"attempt={attempt+1} exit={result.returncode}\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}")
                if result.returncode!=0:raise RuntimeError(f"adapter exit code {result.returncode}")
                if not out.is_file():raise RuntimeError("adapter returned success but output file is missing")
                w,h,depth,color=png_info(out)
                if (w,h)!=(job["width"],job["height"]):raise RuntimeError(f"output dimensions {w}x{h} != job {job['width']}x{job['height']}")
                if (depth,color)!=(8,6):raise RuntimeError(f"output format must be RGBA8 (depth={depth}, color={color})")
                status="succeeded";error=None;break
            except Exception as exc:
                error=str(exc);history.append(f"attempt={attempt+1} error={error}")
        (logs/(jid+".log")).write_text("\n\n".join(history),encoding="utf-8")
        return {"job_id":jid,"asset":job.get("asset"),"output":job["output"],"status":status,
                "error":error,"sha256":sha(out.read_bytes()) if status=="succeeded" else None}
    try:
        results=[]
        with concurrent.futures.ThreadPoolExecutor(max_workers=parallel) as pool:
            futures=[pool.submit(run_one,job) for job in jobs]
            for future in concurrent.futures.as_completed(futures):
                row=future.result();results.append(row)
                print(f"HUD_ARTISAN_JOB {row['status']} id={row['job_id']} output={row['output']}")
        results.sort(key=lambda row:row["job_id"])
        ok=all(row["status"]=="succeeded" for row in results)
        report={"tool":"HUD Artisan","version":VERSION,"jobs_sha256":sha(jobs_path.read_bytes()),
                "adapter":{"provider":provider,"model":model},"parallel":parallel,"results":results,
                "succeeded":sum(row["status"]=="succeeded" for row in results),
                "failed":sum(row["status"]!="succeeded" for row in results)}
        (drop/"reports").mkdir(exist_ok=True)
        (drop/"reports"/"worker-results.json").write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
        if ok:
            generated_at=datetime.datetime.now(datetime.timezone.utc).isoformat()
            job_map={job["job_id"]:job for job in jobs}
            records=[]
            for row in results:
                job=job_map[row["job_id"]]
                records.append({"job_id":row["job_id"],"provider":provider,"model":model,
                    "generated_at":generated_at,"license":license_name,
                    "prompt_sha256":sha(job["prompt"].encode("utf-8")),"output_sha256":row["sha256"]})
            (drop/"provenance.json").write_text(json.dumps({"schema_version":1,"adapter":cfg.get("name",provider),
                "jobs_sha256":sha(jobs_path.read_bytes()),"jobs":records},ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
        print(f"HUD_ARTISAN_DISPATCH {'PASS' if ok else 'FAIL'} jobs={len(results)} succeeded={report['succeeded']} failed={report['failed']} logs={logs}")
        return 0 if ok else 1
    finally:
        shutil.rmtree(work,ignore_errors=True)

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
    q.add_argument("--manifest",type=Path,required=True);q.add_argument("--jobs",type=Path,required=True);q.add_argument("--drop",type=Path,required=True);q.add_argument("--out",type=Path,required=True)
    q.add_argument("--fit",choices=("strict","contain","crop","stretch"),default="strict");q.add_argument("--allow-opaque",action="store_true")
    q=s.add_parser("dispatch",help="run a provider CLI adapter over the jobs with retries and bounded concurrency")
    q.add_argument("--jobs",type=Path,required=True);q.add_argument("--adapter",type=Path,required=True);q.add_argument("--drop",type=Path,required=True)
    q.add_argument("--parallel",type=int)
    q=s.add_parser("validate",help="verify PNGs, atlas, fonts and optional pixel quality heuristics")
    q.add_argument("--manifest",type=Path,required=True);q.add_argument("--pack",type=Path,required=True)
    q.add_argument("--pixels",action="store_true");q.add_argument("--strict-insets",action="store_true");q.add_argument("--engine-contract",action="store_true")
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
             "asset_count_required":92,"commandbuttons_atlas":{"size":[2048,2048],"entries":20},
             "font_files_required":5,"required_unique_assets":ENGINE_ROLES,
             "note":"Contract reference only. Unknown insets, complete filenames, font binaries and source art are intentionally not fabricated."}
            a.out.parent.mkdir(parents=True,exist_ok=True);a.out.write_text(json.dumps(profile,indent=2)+"\n",encoding="utf-8")
            print(f"HUD_ARTISAN_PROFILE_OK contract_only=true roles={len(ENGINE_ROLES)}");return 0
        if a.cmd=="plan":return plan(a.manifest,a.out,a.theme,a.seed,a.engine_contract)
        if a.cmd=="ingest":return ingest(a.manifest,a.jobs,a.drop,a.out,a.fit,a.allow_opaque)
        if a.cmd=="dispatch":return dispatch(a.jobs,a.adapter,a.drop,a.parallel)
        if a.cmd=="validate":return validate(a.manifest,a.pack,a.pixels,a.strict_insets,a.engine_contract)
        if a.cmd=="contact-sheet":return contact_sheet(a.manifest,a.pack,a.out,a.cols,a.cell)
    except (ValueError,RuntimeError,OSError) as e:
        print(f"HUD_ARTISAN_ERROR {e}",file=sys.stderr);return 2
    return 2
if __name__=="__main__":raise SystemExit(main())
