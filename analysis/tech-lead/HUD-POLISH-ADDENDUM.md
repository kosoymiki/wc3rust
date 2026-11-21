# HUD POLISH — addendum to lane 553 (from the operator's review of the direction mocks)

The direction mocks read as retail, but four defects make them a picture, not a HUD. Each has
to be closed in the runtime path (`crates/wc3-game/src/mobile_hud.rs` + `ui/**`), not in the
PIL composite.

## 1. Unit portrait is the letter "A"

Mechanism today: the mock (and the runtime fallback) draws the first glyph of the def name.
Retail shows the unit's **portrait model** (`Units\…\<Name>_Portrait.mdx`) or, when the model
has none, the def's 2D icon (`unitUI` `Art`/`ScoreScreenIcon`). Our data layer already exposes
`wc3_data::units::UnitView::portrait_file()` / `portrait_exists()` and `UnitDef::icon`.

Acceptance:
- `HUD_PORTRAIT def=<id> source=model|icon|placeholder` logged once per def;
- a hero, a building and a worker each show a real portrait at 80 dp (240 px @3×) in the
  runtime `hud_mock` PNG;
- the letter fallback remains only for defs with neither model nor icon, and logs the reason.

## 2. Command card slots are empty

Mechanism today: the mock lays out 12 plates with no icons; the runtime card draws
`CommandButton.icon` but the mock path never populates `commands`.

Acceptance:
- every visible slot draws its ability/unit icon (`unitUI` Art / `AbilityData` Art), states
  normal/pressed/disabled/cooldown/empty distinguishable at 95 px;
- the runtime mock renders a selected worker (build menu), a selected hero (abilities) and a
  building (train/research) with **no empty plate that has a real command behind it**;
- `HUD_CARD slots=<n> icons=<n> missing=<n>` logged, `missing=0` for the Orc01 roster.

## 3. Type scale too small for the panel

Number from the mock: name ≈ 20 px against a 300 px portrait/panel. The direction tokens say
(at density 3×): display 84 px, title 66, heading 60, body 45, label 39, compact 33, numeric 45
tabular. The panel must be composed from those tokens, not from ad-hoc `sp(11, d)` values.

Acceptance:
- name = heading 60 px, class = label 39 px, stats/production = label 39 px, resource numerals
  = numeric 45 px tabular (no width jump when gold goes 9 → 10), tooltip body = body 45 px;
- a single table in the report: element → token → px at 3× → measured px in the PNG.

## 4. Mocks must be runtime renders

The `analysis/design/shots/direction-*.png` files are PIL composites. The proof is
`cargo run -p wc3-game --example hud_mock` producing the real draw path.

Acceptance: runtime PNGs at 2414×1080 for the five selection states (nothing, unit, hero,
building, multi) + tooltip, plus 1080×2414, with `design-qa/scripts/layout_audit.py` rc=0
(0 off-screen, 0 overlap, ≥48 dp targets, text-fit) and a contrast read from the same tokens.
