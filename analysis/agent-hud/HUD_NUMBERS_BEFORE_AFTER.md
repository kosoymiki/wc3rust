# HUD/menu proportions — было → стало (ledger 521)

Measured phone: 2414×1080 px, density 3.0 → 805×360 dp, insets 210/120/90/96 px
= 70/40/30/32 dp. Derived from `HudLayout::compute` and `layout_menu` (single
composer), not from a screenshot guess. Mock artifacts:
`analysis/agent-hud/mock-console-2414x1080.png` (HUD) and
`analysis/agent-hud/mock-menu-campaign.png` (campaign page).

## HUD

| # | item | было | стало |
|---|---|---|---|
| 1 | unit portrait | 61.6 dp (`80 × zone.h·0.55`, zone.h = 112) | **80 dp** (`min(80, zone.h − 4)`) |
| 2 | HP/mana trough width | 383 dp (pre-cap), then 150 dp only in one path | **150 dp** in the one composer, both modes |
| 3 | unit-card info column | 303 dp (383 − 80) with a 150 dp bar floating in it | 376.7 dp; bar capped at 150, text owns the column |
| 4 | minimap frame drawn | 136 dp (112 image + 2 × 12 dp re-expansion in `draw_minimap_frame`) | **112 dp** = `layout.minimap_frame`; image 100 dp (6 dp ring) |
| 5 | minimap art vs hit vs content | three different squares (136 / 112 / 100) | one square (112) + one image (100) |
| 6 | selection info plate | `selection.expanded(4·s)` = 8 px outside the zone | exact `layout.selection` |
| 7 | drawer bottom | 352 px → 24 dp inside the 32 dp gesture inset | 328 px = content bottom (safe) |
| 8 | card drawer | 224 × 176 dp, right edge of the safe content | unchanged size, right-anchored, one source |
| 9 | unit-card rows | name 18 / class 14 / HP 18 / mana 14 / stats 14 / production 14 dp | same heights, zero when the band cannot host 10 dp (no slivers) |
| 10 | bar share of the card | 84 % of the selection width | 41.6 % (150 / 360.7) |
| 11 | toast bands published | always 3, even with 0 live toasts | 0/1/2/3 = live count |
| 12 | tooltip box | centred in the playfield | floated above the hovered slot, clamped to the safe frame |
| 13 | HUD height budget | console 152 dp = 42.2 % + top 30 dp = 50.6 % | unchanged by design; viewport stays 178 dp (49.4 %) |

## Campaign menu

| # | item | было | стало |
|---|---|---|---|
| 14 | title band | 106 px (`content.h × 0.14`, clamped 96..150) | **184 px** (`panel.w 1188 × 0.155`, clamped 84..190) |
| 15 | title type | ~74 px em | ~108 px em (56 % of the band) |
| 16 | tabs | 6 × 178.2 px cells | same 6 cells across the 1113 px content |
| 17 | tab label area | 82 px (30 % reserved for a tag tabs never draw) | **150 px** full plate → "PROLOGUE", "NIGHT ELF" whole |
| 18 | tab label clip | ellipsis at the old floor ("PROLOG", "NIGHT") | whole-word fit, 0.72 × floor hard step, no ellipsis |
| 19 | tabs on a 296 px portrait panel | 6 × 64 px = 408 px, escaped the panel | 2 rows × 3 cells, inside the panel |
| 20 | chapter rows | 6 rows forced at 90 px into a 568 px stack (797 px used, 229 px overflow) | page capacity 2 with the full cutout band; `visible_rows` published for PREV/NEXT |
| 21 | locked chapter row | flat fill `[0.05, 0.045, 0.035]` = colour square | disabled plate at 60 % grey tint, or a bevelled stone plate with a 24 % recessed veil |
| 22 | page label "HUMAN 1/2" | absolute y inside the title rect (overlap on a tall band) | shares the subtitle row, no overlap |
| 23 | row/tab/nav overlap | possible when the stack overflowed | no overlap; capacity is solved before placement |

## Evidence

- `scripts/h2-cargo check -p wc3-game --lib --offline` → `RC=0` (log
  `/data/data/com.termux/files/usr/tmp/lane521-check.log`).
- `scripts/h2-cargo run -p wc3-game --example hud_mock --offline` renders both PNGs
  through the real `MobileHud::draw` / `MobileHud::draw_menu` paths with the
  shipped `human-options-menu-*` / `GlueScreen-*` BLP art.
- Mocks are procedural renders of the shipped art through the real draw path
  (reference captures, not hand-drawn final art).
