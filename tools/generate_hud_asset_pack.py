#!/usr/bin/env python3
"""Generate the original wc3rust Classic + Modern HUD asset pack and ZIP."""
from pathlib import Path
import json, zipfile, shutil

ROOT = Path("hud-asset-pack")
if ROOT.exists():
    shutil.rmtree(ROOT)
for d in ("assets/panels","assets/buttons","assets/frames","assets/icons","assets/menus","atlases","docs"):
    (ROOT/d).mkdir(parents=True, exist_ok=True)

INK="#171512"; DEEP="#211d18"; SHADOW="#090807"; STONE="#51483a"; STONE_HI="#82745b"
GOLD="#c6a15a"; GOLD_HI="#f0d18a"; GOLD_LO="#72552a"
def svg(w,h,body):
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
<defs>
<linearGradient id="stone" x1="0" y1="0" x2="0" y2="1"><stop stop-color="{STONE_HI}"/><stop offset=".22" stop-color="{STONE}"/><stop offset="1" stop-color="#302a22"/></linearGradient>
<linearGradient id="gold" x1="0" y1="0" x2="0" y2="1"><stop stop-color="{GOLD_HI}"/><stop offset=".5" stop-color="{GOLD}"/><stop offset="1" stop-color="{GOLD_LO}"/></linearGradient>
<linearGradient id="wood" x1="0" y1="0" x2="0" y2="1"><stop stop-color="#765236"/><stop offset=".5" stop-color="#49301f"/><stop offset="1" stop-color="#21160f"/></linearGradient>
</defs>{body}</svg>'''
assets=[]
def put(path,w,h,body,states=None,nine=None):
    p=ROOT/"assets"/path
    p.parent.mkdir(parents=True,exist_ok=True)
    p.write_text(svg(w,h,body),encoding="utf-8")
    aid=path[:-4].replace("/"," .").replace(" ","")
    assets.append({"id":aid,"path":f"assets/{path}","format":"svg","width":w,"height":h,
                   "scale_policy":"vector","states":states or ["default"],"nine_slice":nine})

put("panels/panel_base.svg",320,160,f'<rect x="3" y="5" width="314" height="150" rx="9" fill="{SHADOW}"/><rect x="2" y="2" width="316" height="152" rx="8" fill="url(#stone)" stroke="url(#gold)" stroke-width="2"/><rect x="9" y="9" width="302" height="138" rx="5" fill="{DEEP}" stroke="{STONE_HI}"/><path d="M15 24H305M15 137H305" stroke="{GOLD}" opacity=".38"/>',nine={"left":16,"top":16,"right":16,"bottom":16})
put("panels/resource_bar.svg",720,92,f'<rect x="3" y="4" width="714" height="82" rx="9" fill="{SHADOW}"/><rect x="2" y="2" width="716" height="82" rx="9" fill="url(#wood)" stroke="url(#gold)" stroke-width="3"/><rect x="10" y="10" width="700" height="66" rx="5" fill="{DEEP}" stroke="{STONE_HI}"/><path d="M238 17v52M474 17v52" stroke="{GOLD_LO}" stroke-width="2"/><circle cx="27" cy="43" r="7" fill="url(#gold)"/><circle cx="250" cy="43" r="7" fill="#85a85b"/><circle cx="486" cy="43" r="7" fill="#8eafc6"/>')
put("panels/selection_panel.svg",560,210,f'<rect x="3" y="4" width="554" height="202" rx="10" fill="{SHADOW}"/><rect x="2" y="2" width="556" height="202" rx="9" fill="url(#stone)" stroke="url(#gold)" stroke-width="3"/><rect x="10" y="10" width="540" height="186" rx="6" fill="{DEEP}" stroke="{STONE_HI}"/><rect x="18" y="18" width="118" height="118" rx="5" fill="#49301f" stroke="{GOLD_LO}" stroke-width="2"/><rect x="146" y="18" width="394" height="28" rx="3" fill="#49301f"/><rect x="146" y="56" width="394" height="12" rx="3" fill="#302a22"/><rect x="146" y="76" width="394" height="12" rx="3" fill="#302a22"/><rect x="18" y="148" width="522" height="38" rx="4" fill="#49301f" stroke="{GOLD_LO}"/>')
put("panels/command_card.svg",420,330,f'<rect x="3" y="4" width="414" height="322" rx="9" fill="{SHADOW}"/><rect x="2" y="2" width="416" height="322" rx="8" fill="url(#stone)" stroke="url(#gold)" stroke-width="3"/><rect x="9" y="9" width="402" height="308" rx="5" fill="{DEEP}" stroke="{STONE_HI}"/><rect x="16" y="16" width="388" height="28" rx="3" fill="#49301f" stroke="{GOLD_LO}"/>')
put("panels/minimap_frame.svg",260,260,f'<rect x="4" y="5" width="252" height="252" rx="8" fill="{SHADOW}"/><rect x="2" y="2" width="256" height="256" rx="8" fill="url(#stone)" stroke="url(#gold)" stroke-width="3"/><rect x="13" y="13" width="234" height="234" rx="3" fill="#17201a" stroke="{STONE_HI}" stroke-width="2"/>')
put("panels/inventory_strip.svg",400,82,f'<rect x="2" y="3" width="396" height="76" rx="7" fill="url(#stone)" stroke="url(#gold)" stroke-width="2"/><rect x="8" y="9" width="384" height="64" rx="4" fill="{DEEP}"/>')
states={"normal":("url(#wood)","url(#gold)","#d9bd7d"),"hover":("#705033","#f0d18a","#fff0c6"),"pressed":("#2a1c12","#8c6a35","#a98a52"),"selected":("#5b482b","#f0d18a","#f0d18a"),"disabled":("#39342c","#655b49","#80745e")}
for name,(fill,stroke,edge) in states.items():
    put(f"buttons/button_{name}.svg",200,54,f'<rect x="3" y="4" width="194" height="48" rx="6" fill="{SHADOW}"/><rect x="2" y="2" width="196" height="48" rx="6" fill="{fill}" stroke="{stroke}" stroke-width="2"/><rect x="7" y="7" width="186" height="38" rx="3" fill="none" stroke="{edge}" opacity=".55"/><path d="M12 10H188" stroke="{edge}" opacity=".45"/>',states=list(states))
put("frames/frame_corner.svg",48,48,f'<path d="M3 45V12Q3 3 12 3H45L36 12H13Q12 12 12 13V36Z" fill="url(#stone)" stroke="url(#gold)" stroke-width="2"/><path d="M7 39V13Q7 7 13 7H39" fill="none" stroke="{GOLD_HI}" stroke-width="1.5"/><circle cx="12" cy="12" r="3" fill="{GOLD_HI}"/>')
put("frames/divider_ornate.svg",320,18,f'<path d="M2 9H130L150 4L160 9L170 4L190 9H318" stroke="url(#gold)" stroke-width="2" fill="none"/><path d="M145 9l15 -6 15 6 -15 6z" fill="url(#gold)" stroke="{GOLD_LO}"/>')
put("frames/slot_empty.svg",64,64,f'<rect x="2" y="2" width="60" height="60" rx="5" fill="{SHADOW}" stroke="{GOLD_LO}" stroke-width="2"/><rect x="6" y="6" width="52" height="52" rx="3" fill="{DEEP}" stroke="{STONE_HI}"/><path d="M13 13h38v38H13z" fill="none" stroke="{STONE}"/>')
icons={
"gold":'<path d="M32 7C23 17 17 23 17 33a15 15 0 0 0 30 0C47 23 41 17 32 7z" fill="url(#gold)" stroke="#f0d18a" stroke-width="2"/>',
"lumber":'<path d="M12 20l37-8 4 9-37 8zM10 33l38-8 4 10-38 8zM15 46l33-7 3 8-34 7z" fill="#9b7047" stroke="#e0bd82" stroke-width="2"/>',
"food":'<path d="M32 8c-5 8-12 15-12 25a12 12 0 0 0 24 0C44 23 37 16 32 8z" fill="#91a95a" stroke="#d7e39b" stroke-width="2"/>',
"attack":'<path d="M14 48L46 12l7 7-32 36z" fill="#c5c7c8" stroke="#f4e7c6" stroke-width="2"/><path d="M12 43l9 8M17 38l9 8" stroke="#a47a43" stroke-width="5"/>',
"move":'<path d="M32 7l8 17 17 8-17 8-8 17-8-17-17-8 17-8z" fill="#d9c58d" stroke="#fff0c6" stroke-width="2"/>',
"build":'<path d="M9 50V25l23-15 23 15v25H9z" fill="#8d704a" stroke="#e8d1a1" stroke-width="2"/><path d="M26 50V34h12v16M5 25l27-18 27 18" fill="none" stroke="#f0d18a" stroke-width="3"/>',
"repair":'<path d="M20 12a15 15 0 0 0 18 19L50 43l-8 8-12-12A15 15 0 0 1 12 21l9 9 9-9z" fill="#c9b98e" stroke="#f0d18a" stroke-width="2"/>',
"cancel":'<circle cx="32" cy="32" r="23" fill="#6e2d25" stroke="#e5a18e" stroke-width="3"/><path d="M20 20l24 24M44 20L20 44" stroke="#fff0d9" stroke-width="5"/>',
"hero":'<path d="M32 7l7 12 14 4-9 11 1 16-13-6-13 6 1-16-9-11 14-4z" fill="#c7a55c" stroke="#fff0c6" stroke-width="2"/>',
"armor":'<path d="M32 7l19 7v14c0 12-8 20-19 25-11-5-19-13-19-25V14z" fill="#657d83" stroke="#d6d9c9" stroke-width="2"/>',
"health":'<path d="M32 53S8 39 8 23a12 12 0 0 1 24-3 12 12 0 0 1 24 3C56 39 32 53 32 53z" fill="#a83c32" stroke="#f0b5a0" stroke-width="2"/>',
"mana":'<path d="M32 6C23 20 13 30 13 40a19 19 0 0 0 38 0C51 30 41 20 32 6z" fill="#4d79a0" stroke="#c2e0f0" stroke-width="2"/>',
"inventory":'<rect x="11" y="15" width="42" height="34" rx="4" fill="#8a643d" stroke="#e5c78d" stroke-width="3"/><path d="M20 15v-5h24v5M20 25h24" stroke="#f4dfae" stroke-width="3" fill="none"/>',
"settings":'<path d="M32 7l5 5 7-1 2 7 6 4-3 7 3 7-6 4-2 7-7-1-5 5-5-5-7 1-2-7-6-4 3-7-3-7 6-4 2-7 7 1z" fill="#a18a60" stroke="#e9d4a2" stroke-width="2"/><circle cx="32" cy="32" r="8" fill="#27221b" stroke="#e9d4a2" stroke-width="3"/>',
"back":'<path d="M38 12L18 32l20 20M19 32h34" fill="none" stroke="#ead6a4" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/>',
"pause":'<path d="M17 12h10v40H17zM37 12h10v40H37z" fill="#e5d2a1" stroke="#7b5b2d" stroke-width="2"/>',
"sound":'<path d="M9 25h10l14-12v38L19 39H9z" fill="#d8c49a" stroke="#fff0c6" stroke-width="2"/><path d="M40 23q12 9 0 18M45 16q20 16 0 32" fill="none" stroke="#d8c49a" stroke-width="3"/>',
"close":'<path d="M15 15l34 34M49 15L15 49" stroke="#ead6a4" stroke-width="6" stroke-linecap="round"/>'
}
for name,body in icons.items(): put(f"icons/{name}.svg",64,64,body)
put("menus/menu_backdrop.svg",1920,1080,f'<rect width="1920" height="1080" fill="#11100d"/><path d="M0 0H1920V1080H0z" fill="#2c2419" opacity=".35"/><path d="M0 0L620 0 280 1080H0zM1920 0h-620l340 1080h280z" fill="#090807" opacity=".55"/><path d="M0 890Q480 770 960 900T1920 870V1080H0z" fill="#17130f" opacity=".8"/><path d="M90 90H1830V990H90z" fill="none" stroke="{GOLD_LO}" stroke-width="3" opacity=".45"/>')
put("menus/modal_panel.svg",720,480,f'<rect x="8" y="12" width="704" height="460" rx="14" fill="{SHADOW}"/><rect x="4" y="4" width="712" height="460" rx="12" fill="url(#stone)" stroke="url(#gold)" stroke-width="4"/><rect x="16" y="16" width="688" height="436" rx="8" fill="{DEEP}" stroke="{STONE_HI}" stroke-width="2"/><rect x="28" y="28" width="664" height="54" rx="4" fill="url(#wood)" stroke="{GOLD_LO}"/>')
manifest={"pack_id":"wc3rust.hud.classic_modern","version":"1.0.0","schema_version":1,"style":"Classic fantasy RTS materials with modern spacing, contrast and state clarity","license":"CC0-1.0","color_space":"sRGB","alpha":"straight","coordinate_system":"SVG viewBox units; convert logical UI units to framebuffer pixels exactly once","assets":assets}
(ROOT/"manifest.json").write_text(json.dumps(manifest,indent=2),encoding="utf-8")
atlas={"atlas_id":"wc3rust.hud.icons.source","version":"1.0.0","packing":"Source SVGs; rasterize and pack during renderer build/import","padding_px":2,"extrude_px":2,"rotation":False,"entries":[{"id":a["id"],"source":a["path"]} for a in assets if a["id"].startswith("icons.")]}
(ROOT/"atlases/icons.atlas.json").write_text(json.dumps(atlas,indent=2),encoding="utf-8")
(ROOT/"README.md").write_text("""# wc3rust HUD & Menu Asset Pack — Classic + Modern

Version 1.0.0. Original vector-first fantasy RTS UI foundations. No extracted Warcraft III/Blizzard retail artwork is included.

## Contents
- assets/panels: resource bar, selection, command card, minimap and inventory surfaces
- assets/buttons: normal, hover, pressed, selected and disabled states
- assets/frames: corner, divider and empty slot
- assets/icons: original resource, command, hero, status and menu pictograms
- assets/menus: backdrop and modal panel
- manifest.json: stable semantic IDs, dimensions and scale metadata
- atlases/icons.atlas.json: source descriptor, not a prepacked GPU atlas
- docs: integration contract and visual guidelines

## Integration
SVGs are source assets. Rasterize/pack at build time; never decode artwork in the frame loop.
Keep text live/localized. Use stable semantic IDs, not array positions. Draw and hit-test from the same solved UI bounds.
Validate nine-slice values against the renderer convention. Do not infer game actions from icon presence.
No permanent control-group tray is included.

## Scope
Foundational pack, not a final complete game skin. It does not invent unit portraits, spell illustrations, race crests,
item art or map thumbnails without a verified game-content inventory.
""",encoding="utf-8")
(ROOT/"ASSET_LICENSES.md").write_text("""# Asset licenses and provenance

All included SVG artwork and metadata were created specifically for this pack as original vector geometry.
No third-party game files, ripped textures, retail fonts or Blizzard artwork are included.

License: CC0 1.0 Universal for the included artwork and metadata. Use, modify, bundle and redistribute,
including commercially, without attribution. This does not cover external assets added later.
Track source URL, author, exact license and redistribution rights for every later addition.
""",encoding="utf-8")
(ROOT/"docs/VISUAL_GUIDELINES.md").write_text("""# Visual Guidelines
- Structural frame: dark stone, narrow gold trim, restrained bevel.
- Navigation and secondary surfaces: dark timber.
- Content field: deep neutral interior for icon/text contrast.
- Reserve bright gold for hover, focus and selected states.
- Disabled remains legible and distinct from pressed.
- Keep text live; never bake localized labels into images.
- No permanent ten-slot control-group tray.
- Minimap camera/fog/blips must be driven by actual game state.
- Generic pictograms must be mapped to verified game command IDs before shipping.
""",encoding="utf-8")
(ROOT/"docs/INTEGRATION_CONTRACT.md").write_text("""# Integration Contract
1. Validate manifest and required assets at build/startup, never per frame.
2. Reject duplicate IDs, missing files, invalid dimensions and invalid nine-slice insets.
3. Semantic IDs are stable API; never use array positions.
4. UI state owns availability; artwork does not implement game rules.
5. Renderer owns rasterization, atlas packing, texture upload and GPU lifetime.
6. Draw and hit-test use the same solved element bounds.
7. Missing production-critical assets fail validation; development fallback is explicit.
""",encoding="utf-8")
# Validate generated pack before archiving.
ids=[a["id"] for a in assets]
assert len(ids)==len(set(ids))
for a in assets:
    p=ROOT/a["path"]
    assert p.is_file() and "<svg" in p.read_text() and a["width"]>0 and a["height"]>0
archive=Path("wc3rust-hud-asset-pack-classic-modern-v1.0.zip")
if archive.exists(): archive.unlink()
with zipfile.ZipFile(archive,"w",zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for p in ROOT.rglob("*"):
        if p.is_file(): z.write(p,p.relative_to(ROOT.parent))
print(f"Generated {len(assets)} SVG assets; ZIP size {archive.stat().st_size} bytes")
