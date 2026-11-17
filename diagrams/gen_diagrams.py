#!/usr/bin/env python3
"""Generate simple deterministic SVG infographics for wc3rust."""
from pathlib import Path
from xml.sax.saxutils import escape

OUT = Path(__file__).resolve().parent
W, H = 1280, 820
BG = "#0b1220"
PANEL = "#111c31"
BOX = "#1e293b"
BOX2 = "#243449"
TXT = "#e5edf8"
MUTED = "#94a3b8"
ACCENT = "#38bdf8"
GREEN = "#22c55e"
AMBER = "#f59e0b"
RED = "#f87171"


def rect(x, y, w, h, fill=BOX, stroke="#334155", rx=10, sw=1.5, op=1.0):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" opacity="{op}"/>'


def text(x, y, s, size=18, fill=TXT, weight="400", anchor="start", family="monospace"):
    return f'<text x="{x}" y="{y}" font-family="{family}" font-size="{size}" font-weight="{weight}" fill="{fill}" text-anchor="{anchor}">{escape(s)}</text>'


def line(x1, y1, x2, y2, stroke=ACCENT, sw=2, dash=None):
    d = f' stroke-dasharray="{dash}"' if dash else ""
    return f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{stroke}" stroke-width="{sw}"{d} marker-end="url(#arrow)"/>'


def doc(title, body):
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">
<defs>
  <marker id="arrow" markerWidth="10" markerHeight="10" refX="8" refY="3" orient="auto" markerUnits="strokeWidth">
    <path d="M0,0 L0,6 L9,3 z" fill="{ACCENT}"/>
  </marker>
</defs>
<rect width="100%" height="100%" fill="{BG}"/>
{text(40, 52, title, 30, TXT, "700")}
{text(40, 82, "wc3rust — clean-room Rust/Vulkan WC3-compatible RTS · plan 2026-11-12", 15, MUTED)}
{body}
</svg>
'''


def architecture():
    b = []
    layers = [
        ("APPS", "SDL3 window/input/audio · Android/Windows/Linux · CLI tools", 120),
        ("LAUNCHER + UI", "data folder check · profiles · campaign/maps · wgpu UI · safe areas", 190),
        ("INPUT / TOUCH", "pointer router → gesture recognizer → RTS adapter → semantic commands", 260),
        ("SIMULATION", "fixed 20/30 Hz · ECS · orders · pathfinding · combat · fog · replay", 330),
        ("SCRIPTING", "JASS2 VM · triggers · map scripts (sandboxed)", 400),
        ("SCENE", "instances · meshes · materials · animations · particles · culling", 470),
        ("RENDERER", "wgpu/Vulkan pipelines · shadows · terrain · water · UI · pipeline cache", 540),
        ("ASSET DB", "MPQ/CASC/loose → MDX/BLP/SLK → glTF/KTX2 → GPU cache", 610),
        ("PLATFORM", "SDL3 · system Vulkan · Turnip via libadrenotools (optional) · fallback", 680),
    ]
    for title, sub, y in layers:
        b.append(rect(80, y, 1120, 54, BOX))
        b.append(text(100, y + 23, title, 17, ACCENT, "700"))
        b.append(text(250, y + 23, sub, 14, TXT))
    # side arrows
    for y in [174, 244, 314, 384, 454, 524, 594, 664]:
        b.append(line(640, y, 640, y + 16, ACCENT, 2))
    b.append(text(1180, 742, "runtime boundary: no direct file/GPU access from sim", 13, MUTED, anchor="end"))
    return doc("ARCHITECTURE — layers and data flow", "\n".join(b))


def modules():
    b = []
    # clusters
    clusters = [
        ("DATA", 70, 130, 330, 260, "formats · assetdb · tools"),
        ("RENDER", 430, 130, 380, 260, "render · scene · ui · anim"),
        ("SIM", 840, 130, 370, 260, "sim · path · ai · replay"),
        ("SCRIPT", 70, 440, 330, 250, "jass · triggers"),
        ("PLATFORM", 430, 440, 380, 250, "SDL3 · android · touch · input · gpu/drivers"),
        ("QUALITY", 840, 440, 370, 250, "testkit · bench · fuzz"),
    ]
    for title, x, y, w, h, sub in clusters:
        b.append(rect(x, y, w, h, PANEL, "#334155"))
        b.append(text(x + 16, y + 28, title, 18, ACCENT, "700"))
        b.append(text(x + 16, y + 50, sub, 13, MUTED))
    nodes = [
        ("wc3-core", 90, 205), ("wc3-formats", 90, 270), ("wc3-assetdb", 90, 335),
        ("wc3-render", 450, 205), ("wc3-scene", 450, 270), ("wc3-ui", 450, 335),
        ("wc3-sim", 860, 205), ("wc3-path", 860, 270), ("wc3-ai", 860, 335),
        ("wc3-jass", 90, 520), ("wc3-triggers", 90, 585),
        ("wc3-platform", 450, 520), ("wc3-touch", 450, 585), ("wc3-input", 450, 650),
        ("wc3-testkit", 860, 520), ("wc3-replay", 860, 585),
    ]
    for name, x, y in nodes:
        b.append(rect(x, y - 24, 190, 38, BOX2, "#475569"))
        b.append(text(x + 10, y, name, 15, TXT, "700"))
    # edges
    for a, c in [((280, 205), (450, 205)), ((280, 270), (450, 270)), ((280, 335), (450, 335)),
                 ((640, 205), (860, 205)), ((640, 270), (860, 270)), ((640, 335), (860, 335)),
                 ((280, 520), (450, 520)), ((280, 585), (450, 585))]:
        b.append(line(a[0], a[1], c[0], c[1], "#64748b", 1.6))
    b.append(text(40, 760, "dependencies point left→right; core is shared; touch/input never mutates sim directly", 13, MUTED))
    return doc("MODULES — ownership and dependency map", "\n".join(b))


def pipeline():
    b = []
    stages = [
        ("USER DATA", "MPQ · CASC · loose", 80),
        ("INDEX", "source index · listfile", 260),
        ("EXTRACT", "raw files · CRC", 440),
        ("DECODE", "MDX/BLP/SLK/W3X", 620),
        ("TRANSCODE", "glTF · KTX2 · tables", 800),
        ("OPTIMIZE", "ASTC/BC · mips · LOD", 980),
        ("RUNTIME", "asset DB · GPU cache", 1160),
    ]
    for title, sub, x in stages:
        b.append(rect(x, 300, 170, 100, BOX, "#475569"))
        b.append(text(x + 12, 332, title, 15, ACCENT, "700"))
        b.append(text(x + 12, 358, sub, 12, TXT))
    for x in [250, 430, 610, 790, 970, 1150]:
        b.append(line(x - 20, 350, x + 10, 350, ACCENT, 2))
    # side tracks
    b.append(rect(80, 500, 520, 170, PANEL, "#334155"))
    b.append(text(100, 530, "TEXTURE / SHADER TRACK", 16, GREEN, "700"))
    b.append(text(100, 560, "BLP1 → RGBA8 → ASTC/ETC2/BC7 → KTX2", 14, TXT))
    b.append(text(100, 588, "MDX materials → WGSL PBR variants", 14, TXT))
    b.append(text(100, 616, "team color / replaceable IDs as uniforms", 14, TXT))
    b.append(text(100, 644, "pipeline cache, device-loss rebuild", 14, MUTED))
    b.append(rect(640, 500, 560, 170, PANEL, "#334155"))
    b.append(text(660, 530, "XBOX 360 R&D TRACK (SEPARATE)", 16, AMBER, "700"))
    b.append(text(660, 560, "Xenos uCode → HLSL (XenosRecomp) → DXC → SPIR-V", 14, TXT))
    b.append(text(660, 588, "tiled textures → linear · endianness", 14, TXT))
    b.append(text(660, 616, "Xenia/libxemit as research references", 14, TXT))
    b.append(text(660, 644, "not on WC3 PC critical path", 14, MUTED))
    b.append(text(40, 740, "no game assets in repo; user-owned data only; encrypted FreeArc is user-assisted", 13, RED))
    return doc("ASSET PIPELINE — from user data to GPU", "\n".join(b))


def roadmap():
    b = []
    phases = [
        ("P0", "RE · legal · skeleton", "donor matrix · ISO MPQ · triangle", 0, GREEN),
        ("P1", "data + formats", "MPQ/CASC · MDX/BLP · glTF/KTX2", 1, ACCENT),
        ("P2", "RTS vertical slice", "ECS · path · combat · 1 map", 2, ACCENT),
        ("P3", "JASS2 + campaign", "VM · triggers · abilities", 3, ACCENT),
        ("P4", "touch UI + audio", "gestures · launcher · graphics sets", 4, AMBER),
        ("P5", "determinism + perf", "replay hash · 60/120 fps", 5, AMBER),
        ("P6", "multiplayer + release", "LAN lockstep · hardening", 6, RED),
    ]
    x0, y = 70, 280
    b.append(line(x0, y + 40, 1210, y + 40, "#475569", 4))
    for i, (p, title, sub, idx, color) in enumerate(phases):
        x = x0 + idx * 165
        b.append(f'<circle cx="{x}" cy="{y+40}" r="12" fill="{color}"/>')
        b.append(text(x, y + 10, p, 18, color, "700", "middle"))
        b.append(text(x, y + 84, title, 13, TXT, "700", "middle"))
        b.append(text(x, y + 106, sub, 11, MUTED, "400", "middle"))
    b.append(rect(70, 470, 1140, 210, PANEL, "#334155"))
    b.append(text(90, 505, "GATES", 16, ACCENT, "700"))
    gates = [
        "G0 legal: no assets, user data, licenses",
        "G1 data: War3.mpq opens + CRC",
        "G2 formats: MDX/BLP/SLK golden",
        "G3 render: 60 fps · p99 <16.6 ms",
        "G4 sim: replay hash · 1000 units <4 ms",
        "G5 JASS: ≥95% map corpus",
        "G6 touch: p95 <80 ms · 5-user test",
        "G7 perf: cold start/memory/thermal",
        "G8 net: 2–4 LAN · no desync 30 min",
    ]
    for i, g in enumerate(gates):
        col = i % 2
        row = i // 2
        b.append(text(100 + col * 560, 540 + row * 28, "• " + g, 13, TXT))
    b.append(text(40, 740, "target: beat W3AndroidEngine/Warsmash on measured device benchmarks — not claimed until measured", 13, MUTED))
    return doc("ROADMAP — phases and acceptance gates", "\n".join(b))


if __name__ == "__main__":
    (OUT / "architecture.svg").write_text(architecture(), encoding="utf-8")
    (OUT / "modules.svg").write_text(modules(), encoding="utf-8")
    (OUT / "asset-pipeline.svg").write_text(pipeline(), encoding="utf-8")
    (OUT / "roadmap.svg").write_text(roadmap(), encoding="utf-8")
    print("wrote 4 SVGs to", OUT)
