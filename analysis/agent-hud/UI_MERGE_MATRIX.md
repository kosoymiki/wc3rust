# UI layer — merge matrix and port manifest (2026-11-13)

Merge-owner: `ui-research`, ledger **336**, owner paths **`analysis/agent-hud/**`**. This pass is **documentation + verification only**: no source file outside `analysis/agent-hud/` was touched, no device/APK run.

Question this answers: *what of the WC3 UI layer is actually merged into the engine today, what is proven, and what is still missing before the console spec can render?*

---

## 0. Evidence runs (read-only, no device)

| # | Command | rc | Result |
|---|---|---|---|
| 1 | `cargo test -q -p wc3-game --test ui_fdf` | **0** | 10 passed, 0 failed (0.82 s) |
| 2 | `cargo test -q -p wc3-formats --lib fdf` | **0** | 13 passed, 0 failed (47 filtered) |
| 3 | `cargo check -q -p wc3-formats` | **0** | clean |
| 4 | `cargo test -q -p wc3-game --lib ui` | **0** | 16 passed, 0 failed (48 filtered) |

Logs: `$PREFIX/tmp/ui_merge/{ui_fdf,fdf_lib,formats_check,ui_lib}.log`, summary `rc_summary.txt`.
No `cargo` invocation for `wc3-game --lib` as a whole was attempted: other workstreams are editing `hud.rs`/`mobile_hud.rs` concurrently and a full-lib rc would report their in-flight state, not the UI layer's.

---

## 1. Port manifest

Status vocabulary: **merged** (code + tests), **data** (parsed/carried, no consumer), **missing**.

| Artifact | Source (MPQ / donor) | Target (`file:line`) | Status | Proof |
|---|---|---|---|---|
| FDF parser | 70 `*.fdf` under `UI\FrameDef\` | `crates/wc3-formats/src/fdf.rs` | **merged** | run #2 (13 tests); fixture round-trip `fdf.rs:1702-1729` |
| FDF property vocabulary | backdrop/edge/corner file keys | `fdf.rs:443-449` (known props), `:306-308` (`TexCoord` raw order) | **merged** | run #2 |
| Skin table (`war3skins.txt`) | `[Main] Human Orc NightElf Undead Default`; `[Default]` maps `ConsoleTexture01..04` → `HumanUITile01..04` (`war3skins.txt:248-251`) | `crates/wc3-re/src/skin.rs`; `ui.rs:139` `SkinTable`, `:223 get_in`, `:238 resolve_texture`, `:247 resolve_model` | **merged** | `ui_fdf.rs:368 skin_and_localization_tables`, `:379-387` Default/Orc resolution |
| Template inheritance | 1294 `INHERITS` (388 `WITHCHILDREN`, depth 6) | `ui.rs:529 collect_templates`, `:560 template`, `:777 resolve_tree` | **merged** | `ui_fdf.rs:299` asserts 1294/388/6 |
| Anchor layout | 1050 `SetPoint` | `ui.rs:832 build_layout`, `:845 layout_tree`, `:1144 LayoutConfig` | **merged** | `ui_fdf.rs:431/487/549` |
| Console FDF set | `ConsoleUI`, `ResourceBar`, `SimpleInfoPanel`, `InfoPanelUnitDetail`, `UpperButtonBar`, `EscMenuTemplates`, `InfoPanelTemplates`, `GlobalStrings`, `DateStrings`, `NetworkStrings` | `ui.rs:54-65 DEFAULT_DOCUMENTS` | **merged** | runs #1/#4; `ui_fdf.rs:431` (ConsoleUI: 1 frame + **9 texture quads**, 10 declarations, 55 props) |
| Paint emission | FDF `Texture` / `Layer` / `Backdrop` | `ui.rs:1235 PaintKind`, `:1282 LayoutPaint` (`uv[4]`, `color`, `blend`, `layer`), `:1572 paint_order` | **data** | `ui_fdf.rs:520`, `:597` consume paints in tests only |
| 9-slice backdrop fields | `BackdropCornerFlags`, `BackdropCornerSize`, `BackdropBackgroundInsets`, tile/mirror/half-sides | `ui.rs:1244 BackdropPaint`, `:1251 corner_flags`, `:1254 corner_size`, parsed at `:1838-1855` | **data** | fields populated; no renderer consumes them |
| Text, fonts, localisation | `Fonts\*`, `GlobalStrings` + WTS | `ui.rs:522 rebuild_strings`, `FontRegistry`, `:1334 LayoutText`, `:1175 TextBindings` | **merged** | `ui_fdf.rs:642` text shaping, `:763` state bindings |
| Interaction (focus/shortcut/popup/hit) | FDF `ControlShortcut`, `TabIndex`, popup lists | `ui.rs:1318 FrameInteraction`, `:1433 focus_order`, `:1477 shortcut_target`, `:1498 hit_test`, `:1539 hit_test_all` | **merged** | `ui_fdf.rs:614`, `:689` |
| Command / unit icons | 691 `BTN*.blp` | `ui.rs:701 command_icon_paths`, `:706 load_command_icon`, `:748 unit_icon`, `:758 command_icon`; `wc3-re/src/icons.rs:32 audit`, `:84 resolve_icon` | **merged** | 691 files present; atlas/validation from the mobile workstream |
| **Console chrome draw** | `UI\Console\Human\HumanUITile01..04.blp` + `ConsoleUI.fdf` fixed slices | — | **missing** | no consumer of `LayoutPaint` outside tests |
| **9-slice panel draw** | `EscMenuBackground` etc. | — | **missing** | `BackdropPaint` unused at runtime |
| Minimap widget | `UI\MiniMap\*` (20 files) | — | **missing** (HUD-level, not FDF) | `CONSOLE_UI_SPEC.md` §6.1 |
| Tooltip widget | `UI\Widgets\ToolTips\Human\*` | — | **missing** (HUD-level) | §6.11 |
| Time-of-day indicator | `HumanUI-TimeIndicator*` | — | **missing** | §6.10 (clock) |

Corpus numbers the tests pin (70 files): **1 936 declarations, 6 462 properties, 59 `IncludeFile`, 1 294 `INHERITS`, 388 `INHERITS WITHCHILDREN`, 310 unique top-level names, 16 unnamed declarations, 4 distinct include targets, max depth 6**.

---

## 2. Gaps, ranked, with owner and next step

| # | Gap | Impact | Owner | Next step |
|---|---|---|---|---|
| G1 | **No runtime consumer of `LayoutPaint`** — the FDF tree builds rects, text and paints, and nothing draws them (`rg 'paints\(|paint_order'` outside `ui.rs` → 0 hits; only tests call them) | The console cannot render from the merged layer | render/ui draw pass (not this ledger) | Implement the draw pass over `UiLayout::paint_order()`: quads from `LayoutPaint{uv,color,blend}`, then `BackdropPaint` 9-slice, then console fixed-slice chrome (`CONSOLE_UI_SPEC.md` §10.3/§10.4) |
| G2 | **9-slice fields parsed but unused** (`corner_flags`, `corner_size`) | Dialog/tooltip panels would need ad-hoc code | same as G1 | Consume the 8-int `panel_region` contract (§10.3); corners from `corner_size × 0.9` at 1080p |
| G3 | Minimap / tooltip / clock are **not** in the FDF layer | They must be HUD components, not FDF roots | console HUD implementation | Build them per `CONSOLE_UI_SPEC.md` §6.1/§6.11/§6.10 |
| G4 | Command card was **4×2**; `mobile_hud` now ships **4×3** (2026-11-13T07:58Z, `PARITY_MATRIX.md` P4/P8) | Slot count closed; order still heuristic (cost-sorted), no inventory module | mobile/console HUD owners | `CONSOLE_UI_SPEC.md` decision 1; remaining: per-building slot layout + inventory |
| G5 | Phone and console token tables differ (different surfaces) | Risk of copying a phone colour into chrome | doc merge (done here) | §3 precedence: phone = `DESIGN_SPEC.md`, console = `CONSOLE_UI_SPEC.md`; no shared value assumed |

Blockers are **not** external: G1/G2 are implementation work with a written contract, G3/G4 are assigned below, G5 is resolved in this document.

---

## 3. Doc reconciliation — two HUD specs, one rule

`DESIGN_SPEC.md` (phone, portrait + landscape ≤1600) and `CONSOLE_UI_SPEC.md` (landscape ≥1600, console) describe **different surfaces**, so equal token names carry different values by design. Precedence:

```
resolution width < 1600  or orientation == portrait  -> DESIGN_SPEC.md   (glass over 3D)
resolution width >= 1600 and orientation == landscape -> CONSOLE_UI_SPEC.md (chrome over stone)
shared component (resource pill, HP bar, command slot) -> values from the selected profile's table only
```

Delta table for the tokens that exist in both (observed, both files):

| Token | Phone `DESIGN_SPEC.md:19-37` | Console `CONSOLE_UI_SPEC.md` §3.1 | Why they differ |
|---|---|---|---|
| `panel` | `0.043,0.071,0.098 @0.88` (glass) | `#0b1015` (solid backing) | glass card over 3-D vs scrim over chrome |
| `gold` | `0.949,0.757,0.306` (accent) | `#a78900` (sampled trim) | phone accent vs BLP-sampled metal |
| `hp_good` | `0.357,0.890,0.541` | `#41c04a` | phone saturation vs WC3-tinted grey fill |
| `text` | `0.949,0.969,1.000` | `#f2f6ff` | near-identical; console is 1 % cooler |
| `stroke_hot` | `0.400,0.761,1.000` (blue) | `#a78900` (gold) | focus ring colour language differs by profile |
| `stroke` | `0.561,0.706,0.851 @0.24` | `#d0d3d0 @0.24` | neutral hairline vs silver |
| `danger` | `1.000,0.353,0.373` | `#ff5a5f` | equivalent |

Rule applied: **no component mixes tables**. A resource pill on the console uses the console `gold`/`panel`; the same pill on a phone uses the phone table. This is why the two files remain separate rather than merged into one token list.

---

## 4. BASE v0 — definition and status

"BASE v0 green" for the UI layer means all of:

1. 70/70 FDF files load with exact declaration/property counts — **green** (run #1).
2. Theme resolves 4 races + `Default`, including `ConsoleTexture01..04` → `UI\Console\<Race>\<Race>UITile01..04` — **green** (`ui_fdf.rs:379-387`).
3. Console / ResourceBar / SimpleInfoPanel produce anchor-correct rects — **green** (`ui_fdf.rs:431/487/549`).
4. Paint records emitted with uv/colour/blend/layer/backdrop — **green** (data).
5. Hit-test/focus/shortcut/popup tables — **green** (`ui_fdf.rs:614/689`).
6. Text shaping + dynamic bindings (gold/lumber/supply, name/HP/mana) — **green** (`ui_fdf.rs:642/763`).
7. A draw pass that turns (4) into pixels — **RED** (G1).

So BASE v0 is **6/7 green at the data/layout layer**, with the single red being the renderer — exactly the seam this merge hands to the draw-pass owner.

---

## 5. Files changed in this pass

| File | Change |
|---|---|
| `analysis/agent-hud/UI_MERGE_MATRIX.md` | **new** (this document) |
| `analysis/agent-hud/DESIGN_SPEC.md` | +2 lines: precedence pointer to `CONSOLE_UI_SPEC.md` (no content change) |
| `analysis/agent-hud/GITHUB_UI_RESEARCH.md` | §3.1 and §6.2 corrected: Human tiles come from `war3skins.txt` `[Default]` (lines 248-251), `inferred` → `observed` (pinned by `ui_fdf.rs:379-387`) |

No other file was written. Build artifacts: `$PREFIX/tmp/ui_merge/*.log` (outside the repo).

---

## 6. Next steps (owners)

1. **Draw pass** (G1/G2): consume `UiLayout::paint_order()`, `LayoutPaint`, `BackdropPaint`; input = the 8-int panel region and the console chrome contract (§10.3/§10.4 of the spec).
2. **Console HUD module** (G3/G4): `console_hud.rs` per `CONSOLE_UI_SPEC.md` §4–§6, profile P/T, 4×3 card; leave `mobile_hud.rs` untouched.
3. **Art** (G1): export `chrome_{p,t}_{1920,2414}.png` per §10.5 before visual acceptance (T8/T12).
4. Re-run this manifest's commands after 1–2 land; BASE v0 then requires an additional screenshot test, not just rc 0.
