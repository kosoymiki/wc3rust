# PARITY REFS — WC3 PC feature checklist + UI/audio content (2026-11-13)

Ledger **357** (`r2-research` → parity-refs). **No code, no device, owner paths only** (`analysis/agent-hud/**`).
Purpose: a measured checklist of what the PC game *contains*, so parity work can be scored row by row instead of by impression.

**Alignment:** `analysis/tech-lead/PARITY_MATRIX.md` was **not published** at the time of writing (checked 2026-11-13). Rows below use the column order the tech-lead docs already use (`area | feature | evidence | status | gap | owner`), stable `P-###` ids, and no claims about other layers' internals — when `PARITY_MATRIX.md` lands, these rows can be lifted verbatim and the ids keep their meaning.

Status legend:

| Tag | Meaning | Basis |
|---|---|---|
| `BUILD` | layer compiles + tests pass | `BASE.md` §14 (2026-11-13) |
| `DATA` | data parsed by a canonical parser | `BASE.md` §3 |
| `UI` | user-visible screen/control renders | only where evidence exists |
| `MISSING` | no implementation found | my earlier `UI_MERGE_MATRIX.md` G1 |
| `OUT` | deliberate non-goal | `docs/LEGAL.md`, no Battle.net |

---

## 0. Measured content inventory (from the operator's extraction)

| Class | Count | Source (observed) |
|---|---|---|
| Units (incl. creeps/critters) | **465** | `Units/UnitData.slk`, 34 columns |
| — race split | creeps **163**, other 73, human **66**, orc **52**, night elf **47**, undead **47**, critters 8, commoner 5, demon 4 | same, `race` column |
| Abilities | **399** rows × 55 cols | `Units/AbilityData.slk` |
| Items | **131** | `Units/ItemData.slk` |
| Upgrades | **58** rows × 20 cols | `Units/UpgradeData.slk` |
| Destructables | **128** | `Units/DestructableData.slk` |
| Doodads | **275** | `Doodads/Doodads.slk` |
| Terrain tilesets | **12 + `Nothing`** (Ashenvale, Barrens, Blight, Cityscape, Dalaran, Dungeon, Dungeon2, Felwood, LordaeronFall/Summer/Winter, Northrend) | `TerrainArt/` |
| UI FDF documents | **70** (26 in `UI/FrameDef/UI/`, 41 in `Glue/`, 3 string files: `GlobalStrings`, `DateStrings`, `NetworkStrings`) | `UI/FrameDef/**` |
| UI widget assets | **313** files | `UI/Widgets/` |
| Console skins | **4 races** | `UI/Console/{Human,Orc,NightElf,Undead}` |
| Minimap assets | **20** | `UI/MiniMap/` |
| Feedback assets | **79** | `UI/Feedback/` |
| Glue assets | **403** | `UI/Glues/` (BattleNet, Loading, MainMenu, ScoreScreen, SinglePlayer, SpriteLayers) |
| Command icons | **691** BTN BLPs | `ReplaceableTextures/CommandButtons/` |
| Sound files | **1 256** — 1 140 `.mp3`, 95 `.wav`, 15 `.mid`, 6 `.dls` | `Sound/**` |
| — by directory | Dialogue **1 109**, Units 53, Ambient 34, Music 31, Interface 14, Buildings 13, Time 1, Destructibles 1 | same |
| UI sound events | **111** | `UI/SoundInfo/UISounds.slk` (112 rows incl. header) |
| Ability sound rows | **33** | `UI/SoundInfo/AbilitySounds.slk` |
| Campaign archives | **not present** in this extraction (no `Campaigns/`, no `.w3n`) | `unknown` source |

Parsing note (observed): these SLKs store rows as `C;Y<n>;X1;K"id"` followed by **X-only cells** (`C;X2;K…`) that continue the same row. A naive cell regex sees one column and reports "465 IDs"; the row counts above come from a current-row-tracking parser.

---

## 1. Feature checklist — data & content

| id | feature | WC3 evidence | status | gap / note | owner |
|---|---|---|---|---|---|
| P-101 | 4 playable races | 66/52/47/47 units by race | `DATA` | race-specific UI skins exist (4 console dirs) | data/HUD |
| P-102 | Unit roster incl. creeps | 465 units | `DATA` | depends on full SLK columns, not just ids | data |
| P-103 | Ability system | 399 abilities × 55 cols | `DATA` `BUILD` | sim tests green (`merge-sim.md` 99 tests) | sim |
| P-104 | Items | 131 | `DATA` | inventory UI is `MISSING` | data/HUD |
| P-105 | Upgrades | 58 × 20 cols | `DATA` | tech-tree UI `MISSING` | data/HUD |
| P-106 | Destructables / doodads | 128 / 275 | `DATA` `BUILD` | world 2 509/2 509 placed doodads (`BASE.md` §14) | world |
| P-107 | Terrain tilesets | 12 + Nothing | `DATA` `BUILD` | terrain render green | world |
| P-108 | Campaigns | **no archives in extraction** | `MISSING (source)` | needs `War3x.mpq` campaign payload; `unknown` | data |
| P-109 | Neutral buildings / gold mines | creeps 163 incl. neutrals; `GoldMineCollapse*` sounds | `PARTIAL` | spawn/neutral AI not scored here | sim |

## 2. Feature checklist — UI screens and panels

All screens are FDF-driven; today the FDF tree is built and asserted (`ui_fdf` 10/10) but **not rendered** (`UI_MERGE_MATRIX.md` G1: 0 consumers of `LayoutPaint` outside `ui.rs`). So "status" for a screen is `DATA` unless a dedicated HUD path exists.

| id | screen / panel | FDF + assets | status | note |
|---|---|---|---|---|
| P-201 | Main menu | `Glue/MainMenu` (403 glue assets) | `PARTIAL` (M P1) | `Screen::MainMenu` + `menu_view()` exist; no options/credits/profile |
| P-202 | Single player / campaign menu | `Glue/SinglePlayer`, `CampaignMenu.fdf`, `Glue/CampaignMenu.fdf` | `MISSING` | campaign data absent (P-108) |
| P-203 | Map select | `SelectmapScreen` (README APK inventory) | `PARTIAL` (M P2) | `MenuScreen::MapSelect` with START/BACK only; no 44-map list |
| P-204 | Race select | `SelectRaceScreen` (README) | `MISSING` | 4 race skins available |
| P-205 | Loading screen | `Glue/Loading`, `GameSaveSplashDialog.fdf` | `PARTIAL` (M P3) | progress is a 1.1 s timer, not asset loading |
| P-206 | In-game console | `ConsoleUI.fdf` (1 frame + 9 quads), `ResourceBar.fdf`, `SimpleInfoPanel.fdf`, `UpperButtonBar.fdf` | `DATA`; **layout `FULL`** (M P4), **chrome draw `MISSING`** | `mobile_hud` console layout 288 px / 4×3; FDF `LayoutPaint` still has no renderer (`UI_MERGE_MATRIX.md` G1) |
| P-207 | Selection info | `InfoPanelUnitDetail.fdf`, `InfoPanelBuildingDetail.fdf`, `InfoPanelItemDetail.fdf`, `InfoPanelTemplates.fdf` | `PARTIAL` (M P7) | armor/damage only; no attack/armor type, range, hero attributes |
| P-208 | Command card + inventory | `ConsoleUI` fixed wells (4×3 + 2×3) | `PARTIAL` (M P8) | `mobile_hud` ships 4×3 since 07:58Z; slot order is cost-sorted heuristic; no inventory module |
| P-209 | Minimap + control strip | `UI/MiniMap/*` (20), console strip | `PARTIAL` | mobile HUD minimizes; console strip not drawn |
| P-210 | Quest log | `QuestDialog.fdf` (11.8 K) | `DATA` | — |
| P-211 | Allies / alliance dialog | `AllianceDialog.fdf`, `AllianceSlot.fdf` | `DATA` | — |
| P-212 | Chat | `ChatDialog.fdf` | `DATA` | no network (P-218) |
| P-213 | Esc menu / options / save | `EscMenuMainPanel/Options/SaveGame/Templates` | `DATA` | — |
| P-214 | Score screen | `Glue/ScoreScreen` (HumanVictory etc.) | `MISSING` | needs glue flow |
| P-215 | Replay panel | `ReplayPanel.fdf` | `DATA` | replay playback not scored |
| P-216 | Leaderboard / multiboard | `LeaderBoard.fdf`, log dialog | `DATA` | — |
| P-217 | Tooltips / uberTips | `UI/Widgets/ToolTips/**` | `PARTIAL` (M P9) | long-press string only; no border/icon/cost layout |
| P-218 | Battle.net screens | `Glue/BattleNet*` (18 FDFs) | `OUT` | no online services (`docs/LEGAL.md` §2.3) |
| P-219 | Cinematic / suspend / result dialogs | `CinematicPanel.fdf`, `SuspendDialog.fdf`, `GameResultDialog.fdf`, `UnresponsiveDialog.fdf` | `DATA` | — |
| P-220 | Tooltips on world units | `TextTag` font/skin key | `PARTIAL` | needs world-space text pass |
| P-221 | Resource bar (gold/lumber/supply) | `ResourceBar.fdf` 3 columns, icons 32.8 px | `PARTIAL` (M P10) | pills exist; lumber was hard-coded 0 at 07:58Z, supply cap not shown |
| P-222 | Upkeep indicator | `ResourceBarUpkeepText`, upkeep tiers | `PARTIAL` (M P11) | label only, thresholds RoC-wrong (matrix G4) |
| P-223 | 3-D portrait | 223 `*_Portrait.mdx` in the gallery | `PARTIAL` (M P17) | 223/223 renderable, 0 in HUD; icon-only panel today |

## 3. Feature checklist — text & localisation

| id | feature | evidence | status | note |
|---|---|---|---|---|
| P-301 | String tables | `GlobalStrings.fdf`, `DateStrings.fdf`, `NetworkStrings.fdf`; WTS in maps | `DATA` `BUILD` | `ui_fdf` covers strings |
| P-302 | UI font | `Fonts/FRIZQT__.TTF` = **Nimrod MT** (Monotype) | `DATA` | runtime-only, never bundled (`R2_UI_FONT_LICENSING_REFS.md` §2) |
| P-303 | CJK fonts | DFKGothic, Song Light, SquareGothic | `DATA` | `unknown` whether the build needs them |
| P-304 | Cyrillic | Nimrod cmap has `U+0410`; `hud::cyrillic` test | `BUILD` | 1 test (`merge-fdf.md`) |
| P-305 | Numeric formatting | tabular digits 0.5 em; `k` suffix not in WC3 | `n/a` | our own rule (`CONSOLE_UI_SPEC.md` §9) |

## 4. Feature checklist — audio

| id | feature | evidence | status | note |
|---|---|---|---|---|
| P-401 | UI sound events | 111 in `UISounds.slk` (click, error, quest, ping, pause, score tabs) | `DATA`; **runtime `MISSING`** (M P15/P16) | `wc3-audio` complete, `wc3-game` has no dep on it |
| P-402 | Race music playlists | 3 tracks/race + victory/defeat (`war3skins.txt:8-10`, 31 files) | `DATA`; **runtime `MISSING`** (M P15) | engine + `game_audio.rs::music` exist, never started |
| P-403 | Unit voice sets | 53 files in `Sound/Units` + 1 109 in `Sound/Dialogue` | `DATA`; **runtime `MISSING`** (M P16) | `GameAudio::unit_ack` exists, never called |
| P-404 | Ambience | 34 files | `DATA`; **runtime `MISSING`** (M P15) | `AmbienceSounds.slk` 60 rows; no positional loop |
| P-405 | Ability sounds | 33 rows `AbilitySounds.slk` | `DATA`; **runtime `MISSING`** (M P16) | spell SFX not fired |
| P-406 | 3D positional audio | EAX defs, `EnvironmentSounds.slk` | `MISSING` | needs listener/attenuation in `wc3-audio` |
| P-407 | Job-done / upgrade-complete alerts | `JobDoneSound{race}`, `UpgradeComplete{race}` | `DATA`; **runtime `MISSING`** (M P16) | per-race variants parsed, no call site |
| P-408 | MIDI/DLS support | 15 `.mid`, 6 `.dls` | `MISSING` | only if a map uses them |
| P-409 | Voice-over / campaign dialogue | 1 109 Dialogue files | `OUT (no campaign source)` | blocked by P-108 |

## 5. Feature checklist — input, camera, render (reference only)

| id | feature | WC3 evidence | status | note |
|---|---|---|---|---|
| P-501 | Click / drag selection | HUD hit-test exists (`ui_fdf` interaction tests) | `BUILD` | console layout not wired |
| P-502 | Command hotkeys | FDF `ControlShortcut`, `FramehandleNames` | `BUILD` | 19 input tests (`merge-touch.md`) |
| P-503 | Control groups | input layer | `BUILD` | — |
| P-504 | Minimap pan/ping | `MiniMap-Ping.mdx`, `MapPing` sound | `PARTIAL` | — |
| P-505 | Camera | `camera.rs` | `BUILD` | — |
| P-506 | Touch mapping | our addition | `BUILD` | `merge-touch.md` 19/19 |
| P-601 | Team colour | `TeamColor00.blp`, MDX `replaceable_id` 1 | `BUILD` | — |
| P-602 | Fog of war | `CFogState`, minimap fog | `PARTIAL` | — |
| P-603 | Day/night clock | `HumanUI-TimeIndicator*`, `TIME_OF_DAY_HOVER_*` | `MISSING` | UI clock not drawn |
| P-604 | Weather | `UI/Weather*` absent in extraction | `unknown` | — |

---

## 6. UI content references (screen → assets → what it needs)

| Screen | FDF | Textures / models | Dependencies |
|---|---|---|---|
| Console HUD | `ConsoleUI.fdf`, `ResourceBar.fdf`, `SimpleInfoPanel.fdf`, `UpperButtonBar.fdf` | `HumanUITile01..04.blp`, `HumanUI.mdx`, portrait mask | draw pass (G1), 4 race skins |
| Selection detail | `InfoPanelUnitDetail/BuildingDetail/ItemDetail` | `infocard-*.blp` (armor/attack icons) | unit/item data binding |
| Command card | fixed wells in `ConsoleUI` | 691 BTN icons, `human-activebutton`, level overlay | 4×3 grid, states |
| Minimap | `UI/MiniMap/*` | `minimap-{gold,hero,item,neutralbuilding}.blp`, `ping*.blp` | map texture, fog |
| Quest / allies / chat | `QuestDialog`, `AllianceDialog/Slot`, `ChatDialog` | glue backgrounds | screen flow |
| Esc / options | `EscMenu*` (4 FDFs) | `EscMenuBackground` 9-slice | draw pass + settings model |
| Score screen | `Glue/ScoreScreen` | 4 race victory scenes | end-of-game flow |
| Tooltips | `Widgets/ToolTips/Human` | `ToolTipGoldIcon/LumberIcon/SupplyIcon` | text composition + placement |

## 7. Audio content references

| Bucket | Count | Naming | Engine need |
|---|---|---|---|
| UI events | 111 | `InterfaceClick`, `InterfaceError`, `MenuButtonClick`, `GlueScreenClick`, `ScoreScreenTabClick`, `MapPing`, `PauseGame`, `AutoCastButtonClick`, quest set (7), `JobDoneSound{race}`, `UpgradeComplete{race}`, `GoldMineCollapse{race}` | one event → one clip, 2-D |
| Music | 31 | `mp3Music/{Race}1..3`, `{Race}Victory`, `{Race}Defeat` | playlist + crossfade |
| Unit voices | 53 + 1 109 | `Sound/Units/**`, `Sound/Dialogue/**` | 3-D positional, per-clip sets |
| Ambience | 34 | `Sound/Ambient/**` | looping positional |
| Ability sounds | 33 rows | `AbilitySounds.slk` | effect → clip mapping |
| MIDI/DLS | 15 + 6 | `.mid`, `.dls` | optional synth |

Licensing: all audio is Blizzard content — **loaded from the user's archive at runtime, never bundled** (`docs/LEGAL.md` §2.1, `R2_UI_FONT_LICENSING_REFS.md` §1).

---

## 8. Top gaps this checklist exposes (ranked)

| # | Gap | Evidence | Owner | Next action |
|---|---|---|---|---|
| 1 | **No UI draw pass** — every FDF screen is data-only | `UI_MERGE_MATRIX.md` G1 (0 consumers of `LayoutPaint`) | render/ui | implement `paint_order()` consumer; console first (P-206) |
| 2 | **Console chrome not drawn** (mobile HUD ships a 4×3 layout since 07:58Z, but FDF chrome/paint has no renderer) | `PARITY_MATRIX.md` P4 FULL vs `UI_MERGE_MATRIX.md` G1 | HUD/FDF | draw pass + `console_hud.rs` per spec §4–§6 |
| 3 | **Campaign content absent** from the extraction | no `Campaigns/`, no `.w3n` | data | confirm archive provenance before scoring P-108 |
| 4 | **3-D positional audio missing** | `wc3-audio` has decode, no listener model | assets/audio | listener + attenuation + EAX mapping (P-406) |
| 5 | **Day/night clock** not drawn | `HumanUI-TimeIndicator*` present, no consumer | HUD | clock component per spec §6.10 |
| 6 | Inventory / item UI absent | 131 items, no inventory screen | HUD | 2×3 module (spec §6.7) |
| 7 | Tooltips / ubertips not placed | `Widgets/ToolTips` data only | HUD | tooltip component (spec §6.11) |

## 9. References

| # | Source | Access | Use |
|---|---|---|---|
| 1 | Local extraction `analysis/war3_full/**` | 2026-11-13 | all counts in §0 (primary, byte-level) |
| 2 | `analysis/tech-lead/BASE.md` §3, §13, §14 | 2026-11-13 | status tags `BUILD`/`DATA`, gates |
| 3 | `analysis/tech-lead/WC3RE_MERGE_MATRIX.md` | 2026-11-13 | canonical parser per data class |
| 4 | `analysis/tech-lead/DISPATCH_STATUS.md` | 2026-11-13 | layer ownership |
| 5 | `analysis/agent-hud/UI_MERGE_MATRIX.md` | 2026-11-13 | G1 gap, port manifest |
| 6 | `analysis/agent-hud/FONT_TEXT_METRICS.md`, `R2_UI_FONT_LICENSING_REFS.md` | 2026-11-13 | font/licence facts |
| 7 | `analysis/tech-lead/PARITY_MATRIX.md` | **absent** 2026-11-13 | alignment target — rows ready to lift |

## 10. Open items

1. ~~PARITY_MATRIX.md not published~~ — **published 2026-11-13T07:58Z**; alignment is §11 below. `P-###` ids stay stable for the fold into matrix v2 (§8.5 of the matrix already lists this document).
2. **Campaign payload** source unknown in this extraction (P-108) — blocks 1 109 dialogue assets and the campaign menu row.
3. **Status of sim/render rows** is borrowed from `BASE.md` §14; this ledger does not re-run those gates.
4. **Audio 3-D** design not specified anywhere yet — needs its own small spec before implementation.

---

## 11. Alignment with `PARITY_MATRIX.md` (authoritative, snapshot 2026-11-13T07:58Z)

The matrix is the scorecard; this document is the content reference behind it. Mapping of matrix §4 rows to rows here, with the matrix status adopted (where the two disagreed, the matrix wins and the row above was corrected in this pass):

| Matrix row | Matrix feature | Matrix status | Rows here | Note |
|---|---|---|---|---|
| P1 | Main menu | PARTIAL | P-201 | menu screen exists; no options/credits/profile |
| P2 | Map selection | PARTIAL | P-203 | 44 maps discovered, list not rendered |
| P3 | Loading screen | PARTIAL | P-205 | timer, not asset progress |
| P4 | Console layout | FULL | P-206 | 288 px band, 4×3 cells; **layout** is full, FDF chrome **draw** is still absent (G1) |
| P5 | Minimap | PARTIAL | P-209 | dots/camera/frame; no terrain image, no fog |
| P6 | Portrait | PARTIAL | P-207 | icon-only panel |
| P7 | Stats panel | PARTIAL | P-207 | armor/damage only |
| P8 | Command card | PARTIAL | P-208 | 4×3 since 07:58Z; order heuristic; no inventory |
| P9 | Tooltips | PARTIAL | P-217 | long-press string only |
| P10 | Resource bar | PARTIAL | P-221 | lumber/supply gaps |
| P11 | Upkeep indicator | PARTIAL | P-222 | label only |
| P12 | Selection groups | PARTIAL | P-503 | input exists, no on-screen row |
| P13 | Hotkeys | FULL | P-502 | letters generated from labels |
| P14 | Fog of war (visual) | MISSING | P-602 | state exists, no consumer |
| P15 | Music / ambience | MISSING | P-402, P-404 | `wc3-audio` complete, not a `wc3-game` dep |
| P16 | Unit acks / spell sfx | MISSING | P-403, P-405, P-407 | call sites absent |
| P17 | 3-D portraits | PARTIAL | P-223 | gallery renders 223, HUD renders 0 |

### 11.1 Conflicts found and resolved in this pass

| Conflict | Before | Resolution |
|---|---|---|
| Doodads count | this doc 275; matrix 278 | both true: 278 Y-slots (`maxY 279`, 3 empty), **275 non-empty rows**; matrix value adopted in §0 with the empty-row note |
| Console status | this doc `DATA` (`MISSING` draw) vs matrix `FULL` | split: **layout FULL** (mobile_hud 288 px / 4×3) vs **FDF chrome draw MISSING** — different mechanisms, both statements now scoped |
| Command card | this doc said "mobile HUD is 4×2" | stale: `mobile_hud` ships **4×3** since 07:58Z (matrix P4/P8); §2 row and `CONSOLE_UI_SPEC.md` decision 1 corrected |
| Audio | this doc `DATA` (library parsed) | runtime is **MISSING** (`wc3-game` has no `wc3-audio` dep); rows now say `DATA; runtime MISSING` |
| Menu/loading screens | this doc `MISSING` | tree gained `Screen::{MainMenu,MapSelect,Loading}` during the merge → `PARTIAL` |

### 11.2 Rows only this document scores (candidates for matrix v2)

`P-108` campaigns (matrix G23/G24 cover the feature, not the content source), `P-210` quest log, `P-211` alliance, `P-212` chat, `P-213` esc/options, `P-214` score screen, `P-215` replay, `P-216` leaderboard, `P-218` Battle.net (`OUT`), `P-219` cinematic/suspend/result, `P-301..P-305` text/localisation, `P-408` MIDI/DLS, `P-604` weather. The matrix's §8.5 lists this file as a sibling to fold in; §11 is the fold map.
