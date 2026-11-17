#!/usr/bin/env python3
"""Build layered PSDs from the HUD redraw pack (pytoshop).

Outputs (next to the pack):
  herofolends_hud_landscape.psd  - 1920x1080 composition with named groups/layers
  herofolends_hud_components.psd - every manifest PNG as its own named layer (grid)
"""
import json
import os
import numpy as np
from PIL import Image
from pytoshop import enums
from pytoshop.user import nested_layers as nl

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "analysis/agent-hud/redraw-v0.1/herofolends-hud-redraw")
OUT = os.path.dirname(PACK)


def image_layer(path, name, left=0, top=0, width=None, height=None, visible=True):
    im = Image.open(path).convert("RGBA")
    if width and height and (im.width != width or im.height != height):
        im = im.resize((width, height), Image.LANCZOS)
    r, g, b, a = [np.array(c) for c in im.split()]
    return nl.Image(name=name, top=int(top), left=int(left), visible=visible,
                    channels={0: r, 1: g, 2: b, -1: a})


def solid_layer(name, w, h, rgba=(26, 22, 18, 255)):
    ch = [np.full((h, w), v, dtype=np.uint8) for v in rgba]
    return nl.Image(name=name, top=0, left=0, channels={0: ch[0], 1: ch[1], 2: ch[2], -1: ch[3]})


def p(name):
    return os.path.join(PACK, name)


def landscape():
    W, H = 1920, 1080
    groups = []
    groups.append(nl.Group(name="00_background", layers=[solid_layer("stone", W, H)]))
    groups.append(nl.Group(name="10_console", layers=[
        image_layer(p("console/console_left_cap.png"), "console_left_cap", 0, 792, 256, 288),
        image_layer(p("console/console_left_mid.png"), "console_left_mid", 256, 792, 704, 288),
        image_layer(p("console/console_right_mid.png"), "console_right_mid", 960, 792, 704, 288),
        image_layer(p("console/console_right_cap.png"), "console_right_cap", 1664, 792, 256, 288),
    ]))
    groups.append(nl.Group(name="20_minimap", layers=[
        image_layer(p("minimap/minimap_header.png"), "minimap_header", 24, 812, 216, 20),
        image_layer(p("minimap/minimap_frame_normal.png"), "minimap_frame", 24, 840, 216, 216),
    ]))
    groups.append(nl.Group(name="21_portrait", layers=[
        image_layer(p("portrait/portrait_frame_normal.png"), "portrait_frame", 264, 832, 128, 128),
    ]))
    groups.append(nl.Group(name="22_selection", layers=[
        image_layer(p("selection/selection_panel.png"), "selection_panel", 264, 792, 240, 180),
    ]))
    card = []
    for row in range(3):
        for col in range(4):
            card.append(image_layer(p("commandcard/button_border_normal.png"),
                                    f"slot_{row*4+col:02}", 1628 + col*68, 856 + row*68, 64, 64))
    groups.append(nl.Group(name="30_commandcard", layers=card))
    res = []
    for i, state in enumerate(("normal", "normal", "normal")):
        res.append(image_layer(p(f"resources/resource_pill_{state}.png"), f"pill_{i}", 1176 + i*242, 796, 236, 32))
    for i, icon in enumerate(("gold", "lumber", "food")):
        res.append(image_layer(p(f"resources/icon_{icon}_normal.png"), f"icon_{icon}", 1188 + i*242, 802, 32, 32))
    res.append(image_layer(p("resources/upkeep_roc_0.png"), "upkeep_roc_0", 1176, 834, 720, 14))
    groups.append(nl.Group(name="40_resources", layers=res))
    groups.append(nl.Group(name="50_topbar", layers=[
        image_layer(p("topbar/topbar_plate.png"), "topbar_plate", 48, 80, 720, 32),
    ]))
    groups.append(nl.Group(name="60_tooltip", layers=[
        image_layer(p("tooltip/tooltip_panel.png"), "tooltip_panel", 1440, 560, 384, 256),
        image_layer(p("tooltip/tooltip_arrow_down.png"), "tooltip_arrow", 1632, 816, 24, 12),
    ]))
    groups.append(nl.Group(name="70_cursor_selection", layers=[
        image_layer(p("cursor/selection_circle_hero.png"), "selection_circle_hero", 960, 520, 96, 96),
        image_layer(p("cursor/selection_circle_unit.png"), "selection_circle_unit", 900, 560, 96, 96),
        image_layer(p("cursor/cursor_target.png"), "cursor_target", 1200, 400, 48, 48),
        image_layer(p("cursor/cursor_move.png"), "cursor_move", 1256, 400, 48, 48),
        image_layer(p("cursor/cursor_attack.png"), "cursor_attack", 1312, 400, 48, 48),
        image_layer(p("cursor/rally_point.png"), "rally_point", 1368, 400, 32, 32),
        image_layer(p("cursor/order_marker.png"), "order_marker", 1408, 400, 32, 32),
    ]))
    cg = [image_layer(p("build/build_progress.png"), "build_progress", 1700, 1048, 64, 8)]
    for i in range(10):
        cg.append(image_layer(p("controlgroups/control_group_button_inactive.png"),
                              f"control_group_{i}", 48 + i*50, 740, 44, 44))
    groups.append(nl.Group(name="80_build_controlgroups", layers=cg))
    menus = [
        image_layer(p("menus/menu_panel.png"), "menu_panel", 704, 348, 512, 384, visible=False),
        image_layer(p("menus/menu_button_normal.png"), "menu_button", 832, 760, 256, 64, visible=False),
        image_layer(p("menus/loading_frame.png"), "loading_frame", 448, 284, 1024, 512, visible=False),
        image_layer(p("menus/score_panel.png"), "score_panel", 448, 284, 1024, 512, visible=False),
    ]
    groups.append(nl.Group(name="90_menus_hidden", layers=menus, visible=False))
    psd = nl.nested_layers_to_psd(groups, color_mode=enums.ColorMode.rgb,
                                  compression=enums.Compression.raw, size=(H, W))
    out = os.path.join(OUT, "herofolends_hud_landscape.psd")
    with open(out, "wb") as f:
        psd.write(f)
    return out


def components():
    manifest = json.load(open(os.path.join(PACK, "manifest.json")))
    assets = [a for a in manifest["assets"] if a.get("file", "").endswith(".png")]
    cell_w, cell_h, cols = 300, 340, 8
    rows = (len(assets) + cols - 1) // cols
    W, H = cols * cell_w, rows * cell_h
    layers = [solid_layer("sheet_background", W, H, (18, 16, 14, 255))]
    for i, a in enumerate(assets):
        col, row = i % cols, i // cols
        layers.append(image_layer(p(a["file"]), a["name"], col * cell_w + 16, row * cell_h + 16,
                                   a["w"], a["h"]))
    psd = nl.nested_layers_to_psd(layers, color_mode=enums.ColorMode.rgb,
                                  compression=enums.Compression.raw, size=(H, W))
    out = os.path.join(OUT, "herofolends_hud_components.psd")
    with open(out, "wb") as f:
        psd.write(f)
    return out


if __name__ == "__main__":
    a = landscape()
    b = components()
    print("landscape", a, os.path.getsize(a))
    print("components", b, os.path.getsize(b))
