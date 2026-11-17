# P0 HUD — scale +10%, panel clamping, minimap anchoring (ledger 347)

**Art honesty:** `analysis/agent-hud/redraw-v0.1` is a **procedural reference v1** pack, not final
hand-drawn Blizzard art. It is labelled as such in code (`RedrawSkin` doc, `mobile_hud.rs:545`) and here.

## 1. Toolchain audit (command -v / --version)

| Tool | Version | rc/source |
|---|---|---|
| cargo | 1.99.1 (5f94df478 2026-08-27) | present |
| rustc | 1.99.1 (58cf8f9c3 2026-10-13) | present |
| python3 | 3.14.10 | present |
| pip | 26.3 | present |
| node / npm | v24.22.2 / 12.2.2 | present |
| bun | 1.4.4 | present |
| radare2 | 6.2.6 | present |
| rtk | 1.1.0 | present |
| ffmpeg / sox | 9.0.2 / SoX_ng 14.8.1 | present |
| ImageMagick | 7.1.2-35 Q16-HDRI | present |
| optipng / pngquant | 7.9.1 / 3.0.3 | present |
| @gltf-transform/cli | 4.5.1 | present |
| assimp | library `libassimp.so` 9.9M + headers | `pkg install assimp` rc 0 — **no CLI binary** in the Termux package (gap) |
| stormlib | — | `pkg install stormlib` rc 100 (not in Termux repo); MPQ is covered by the project's own `wc3-mpq` crate |
| trimesh / pygltflib | 5.2.1 / 1.16.5 | `pip install --user` rc 0 |
| three | 0.187.0 | `npm i -g three` rc 0 (global, not bare-importable without NODE_PATH) |
| Pillow | 12.3.0 | present |

Image-tool usage for the 9-slice assets (evidence): `magick` + `optipng` generated
`analysis/agent-hud/redraw-v0.1/blp-verify/console-9slice-guide.png` (256×288, inset lines at
32/48/32/48; `magick identify` → PNG 256x288; optipng rc 0).

## 2. H2 base used

- `harness2_corpus_search` hit **61936** (`dream.acceptance-qa-must-follow-the-canonical-hud-not-4316`):
  acceptance must follow the canonical console HUD, not the old mobile gate.
- `harness2_experience` search hit **4439**: judge HUD scale/overflow/anchor on device evidence; never
  present procedural/auto-sliced art as finished hand-drawn art — label it `procedural reference vN`.
  This report and the code doc use that label.

## 3. Fixes (file:line)

### (1) HUD_SCALE_BOOST = 1.10
- `crates/wc3-game/src/mobile_hud.rs:19` `pub const HUD_SCALE_BOOST: f32 = 1.10;`
- Layout scale: `mobile_hud.rs:957-975` — `base = min(vw,vh)/1080 * UiScale * 1.10`, clamped
  `0.62..2.0`, then `min(probe_height * 0.6 / CONSOLE_HEIGHT)` and `min(probe_height / CONSOLE_HEIGHT)`
  so the boosted console always fits the safe area; safe insets recomputed with the final scale.
- Menus use the same boost: `mobile_hud.rs:1661` (`layout_menu`), `:1693` (`draw_menu`).

### (2) Text/icon rects clamped to their panels
- `mobile_hud.rs:556` `clamp_rect(rect, parent)` (intersection; zero-sized when disjoint).
- Selection (portrait icon, name, class, HP, mana, stats, production): `mobile_hud.rs:1795, 1811,
  1820, 1841, 1853, 1873, 1884` — all clamped to `layout.portrait`/`layout.console`.
- Selection panel width stops before the command card (portrait overflow found by the new test):
  `mobile_hud.rs:1032-1044` `selection_w = min(info_x+info_w-portrait.x, card.x-8s-portrait.x)`.
- Resource pills (icon, value, label): `mobile_hud.rs:1435, 1451, 1468` clamped to each `pill_rect`.
- Card slots (icon, hotkey, sub): `mobile_hud.rs:2014, 2038, 2056` clamped to the slot rect.
- Top bar, upkeep, hint, toast, tooltip, minimap header: `clamp_rect(... layout.top_meta/console/
  hint/toast/tooltip/minimap_header)`.
- Group-row labels now `text_box` inside the slot rect (`game.rs:1429-1452`).

### (3) Minimap anchoring
- Retail chrome and pack frame both drawn at **exactly `layout.minimap`** (no `expanded` drift):
  `mobile_hud.rs:1337` ("frame is the hit rect") and `:1347` (216×216 authored frame at `map`).
- Header anchored to `layout.minimap_header`, which shares x/w with `layout.minimap`.

## 4. Gates (canonical only)

| Command | rc | Result |
|---|---:|---|
| `cargo check -p wc3-game --lib --offline` | **0** | 0 errors, 50 warnings |
| `cargo test -p wc3-game --lib --offline hud::` | **0** | **23 passed, 0 failed** (`analysis/agent-hud/p0-hud-scale-tests2.log`) |

New tests:
- `hud_scale_boost_is_applied_and_clamped` — +10% at 2414×1080, console inside the safe area at
  2414×1080 / 1080×2414 / 1280×720 / 640×360.
- `minimap_frame_and_header_are_anchored_to_the_hit_rect` — centre + all four frame edges hit
  `HudHit::Minimap`; header x/w match the minimap.
- `text_and_icon_rects_stay_inside_their_panels` — selection texts inside the console, slots inside
  the card, pills inside resources; every font vertex inside the safe frame (+2 px) at both
  orientations.

Regenerated headless artifact (boost + clamps applied): `analysis/agent-hud/mock-redraw-2414x1080.png`
(2414×1080, procedural reference v1).

## 5. Not done (gated)
- Device screenshot **not run**: `H2_DEVICE_SLOT` is not `1`; no `am start`/`monkey`/install.
- assimp CLI and stormlib are unavailable as binaries (gaps above); `libassimp.so` + headers are
  installed for future C/Rust bindings.

## Increment: tooltip pack panel (procedural reference v1)

- `crates/wc3-game/src/mobile_hud.rs` — `nine_slice_quads(rect, edge_px, uv_edge)` helper
  (fixed corners/edges, stretched centre; returns 9 `(rect, uv)` pairs).
- Tooltip layout: the pack shell (384×256, 24 px insets) now gets a **96*scale tall box**
  (retail pair keeps the 30*scale strip), bottom anchored above the console.
- Tooltip draw: the pack panel is drawn as a 9-slice with `uv_edge = 24/384, 24/256` instead of one
  stretched quad (corners no longer squashed).
- Test `tooltip_pack_panel_uses_a_tall_nine_sliced_box`: retail strip height, pack box height,
  safe-frame containment, 9-slice coverage (sum of parts == box area) and UV bounds.

## Final gates (this packet)

| Command | rc | Result |
|---|---:|---|
| `cargo check -p wc3-game --lib --offline` | **0** | 0 errors |
| `cargo test -p wc3-game --lib --offline hud::` | **0** | **24 passed, 0 failed** — `analysis/agent-hud/p0-hud-final-tests.log` |

Device screenshot remains gated: `H2_DEVICE_SLOT` is not `1`; no launch attempted.
