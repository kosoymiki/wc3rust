# DragonUI Donor Intake (byte-level)

**Date:** 2026-11-13  
**Donor:** `~/DragonUI-20261113` (read-only, private project donor; in-scope RE per H2)  
**Task:** `r2-assets` ledger 351, ROUND 6 p1  
**Purpose:** Full byte-level intake and per-mechanism adopt/adapt/reject verdicts to port strongest mechanisms (layout, atlases, font system, scaling) into herofolends-rs.

## 1. Byte inventory (observed)

| Metric | Observed |
|---|---:|
| Total files | 1,107 |
| Total size | 254M (`du -sh ~/DragonUI-20261113`) |
| Top-level | `.git/`, `.github/`, `DragonUI/`, `DragonUI_Options/`, `.gitattributes`, `.gitignore`, `CONTRIBUTING.md`, `LICENSE`, `README.md` (21.7K) |
| `.blp` | 418 |
| `.lua` | 341 |
| `.tga` | 243 |
| `.xml` | 37 |
| `.ttf` | 2 (`DragonUI/Fonts/expressway.ttf` 116.6K; `DragonUI_Options/fonts/` empty) |
| Other | 16 txt, 14 sample, 5 md, 4 jpg, 3 yml, 2 toc, 2 py, 1 rev/pack/idx/gitignore/gitattributes, `LICENSE` no-ext |

### Directory structure

**DragonUI/** (addon core): `Fonts/`, `LICENSES/`, `Locales/` (11 lua), `Textures/` (27 subdirs: ActionBars, Bags, Castbar, CharacterPanel, ClassIcons, Coins, Collections, Editmode, ForeverUI, Mail, Merchant, Micromenu, Minimap, Nameplates, NewLevelUp, PersonalResource, Professions, Quest, RacePortraits, Reputation, Spellbook, SwingTimer, Talents, Taxi, UI, UnitFrames, WorldMap, XP), `core/` (12 lua + core.xml), `data/` (4 lua: aura_durations 226.5K, companion_traits 11.4K, mount_speeds 6.4K, npc_rare_ranks 97.2K), `libs/` (LibKeyBound-1.0 etc.), `modules/` (23 dirs, 50+ module lua incl characterpanel 31, worldmap 18, unitframes 18, talents 14, spellbook 13, nameplates pipeline/features/core 27, actionbars 6, bagster 8, professions 14, merchant 4, collections 4, etc.), `utils/` (14 lua), root `DragonUI.xml`, `config.lua`, `embeds.xml`.

**DragonUI_Options/**: `LICENSES/`, `Locales/` (10), `Textures/`, `fonts/` (empty), `libs/` (AceGUI-3.0 widgets 22), `panel/` (21 lua), option TOC/xml.

**Texture counts by subdir (observed `find ... -type f | wc -l`):** UI 91, CharacterPanel 36, Spellbook 32, UnitFrames 25, Talents 24, ActionBars 23, XP 22, Nameplates 21, Minimap 18, ForeverUI 17, WorldMap 16, Reputation 15, Micromenu 14, Collections 12, ClassIcons 11, Editmode 11, Professions 10, Quest 9, Castbar 7, Bags 7, Taxi 5, Merchant 4, PersonalResource 4, NewLevelUp 4, RacePortraits 4, Mail 3, SwingTimer 2, Coins 1.

### Load/boot architecture (observed)
- `DragonUI/DragonUI.xml`: loads fonts early, then config/embeds/modules via includes.
- `DragonUI/core/core.xml` load order: `api.lua` → `reload.lua` → `texture_warmup.lua` → `modelview.lua` → `editor_settings.lua` → `editor_panel.lua` → `position_presets.lua` → `commands.lua` → `visibility_fade.lua` → `combatlog_fix.lua` → `uiscale.lua` (note: `fonts.lua` loads earlier via `DragonUI.xml`).
- `DragonUI/modules/modules.xml`: `<Include file=".../module.xml"/>` + `<Script file="..."/>` factory per module.
- `DragonUI/config.lua`: defines `addon._dir` (addon texture path root) used by texture warmup/atlas resolution.
- 2 TTF total in repo; Options fonts dir empty.

## 2. Mechanism matrix — per-mechanism verdicts

Verdict key: **ADOPT** = port mechanism directly (adapted to Rust/wc3 stack), **ADAPT** = port concept/pattern with project-specific implementation, **REJECT** = not applicable / not worth porting now.

| Mechanism | Evidence (file:line/symbol, observed) | Donor behavior | Verdict | Port target / notes |
|---|---|---|---|---|
| **Layout: position persistence (quadrant anchors)** | `core/position_presets.lua` (helpers `PickFields/CopyTable/MergeFields/SaveQuadrantSection`, field maps `TOTEM_POSITION_KEYS`/`STANCE_POSITION_KEYS`, `addon.GetQuadrantAnchor` → stores `anchor, x, y` per section in `addon.db.profile[sectionName]`; subtracts `DragonUI_LayoutOffset` so auto-shifts not saved; uses `AceSerializer-3.0` + `LibDeflate`) | Per-module saved positions as quadrant-relative (anchor + x/y) with offset compensation and serialization | **ADAPT** | Port quadrant-anchor + layout-offset compensation into our HUD/layout state (Rust structs/serde or existing profile store) so user moves persist independently of resolution/layout shifts. Do not import Lua/LibDeflate; implement native serialization. Apply to HUD modules once HUD owner wires. |
| **Layout: edit-mode editor panel** | `core/editor_panel.lua` (constants `DIALOG_WIDTH=395`, `COMPACT_ROW_WIDTH=320`, `ROW_WIDTH=343`, `ROW_HEIGHT=32`, `ROW_GAP=2`, `LABEL_WIDTH=118`, coord/reset sizing `COORD_BOX_WIDTH/AXIS_GAP/RESET_SIZE`, strata `EditorUI` layers, selection tint helpers, slider/coord rows, compact dialogs, `SCREEN_MARGIN/SIDE_GAP/SCROLL_ROOM`) | Data-driven editor UI for moving/scaling frames with compact/wide layouts, coord stepping, reset, selection tint | **ADAPT** | Adopt layout math/patterns (row sizing, coord+reset affordance, compact threshold, scroll room) for our in-game HUD editor/debug overlay. Keep rendering in our UI stack; no WoW frame API. Useful for parity HUD placement tooling. |
| **Layout: module/editor registration** | `core/editor_settings.lua` (`E.Register("PetFrame"/"party", { settings = Join(...) })`, sliders/checks/drops/sections/text rows, per-module `resize`, `hidden` predicates, `R.pet/R.party` rebuild callbacks), `core/commands.lua` (`/dragonui`, `/dui`), `modules/modules.xml` Include/Script factory | Declarative per-module editable settings + slash commands + XML module composition | **ADAPT** | Adopt declarative module-setting schema idea (id, range, visibility predicate, rebuild/apply hook) for HUD config; implement in Rust/data, not Lua. Reject WoW-specific command wiring. |
| **Atlases: named atlas registry + lookup** | 17+ modules reference `addon.atlasinfo` (e.g. `core/texture_warmup.lua:72-74` warmup by class/`atlasinfo`; `modules/spellbook/book_controls.lua:70,498` `spellbook-corner-flipbook-evergreen`, `redbutton-highlight`; `modules/spellbook/book_flyout.lua:37`; `modules/talents/window_shell.lua:100` `backgroundAtlas`; `modules/talents/build_menu.lua:21` `common-dropdown-textholder`; `modules/characterpanel/{sidebar.lua:756,honorpane.lua:127}`; `modules/loot_skin.lua:455`) | Central atlas metadata table keyed by name → sheet/rect/UV info consumed across UI modules for slicing/highlights/backgrounds | **ADAPT** | Port concept as a named UI-atlas/sprite-sheet registry (name → texture path/rect/9-slice metadata) over our existing BLP/texture pipeline (`wc3-assetdb` textures, `TextureAsset` mips). Do **not** copy WoW atlas data; build registry from our UI assets. Enables consistent button/highlight/background reuse. |
| **Atlases/textures: batch texture warmup** | `core/texture_warmup.lua:1-120` (per-window measured file lists e.g. characterpanel `CharacterPanel\*`, `UI\*`, `WorldMap\*`; `add(rel) files[#files+1]=addon._dir..rel`; class-gated warmup via `atlasinfo`) | Preload first-open UI textures with world load to avoid stall; path built from `addon._dir` | **ADAPT** | Port warmup/preload strategy: queue known HUD/UI texture paths into `AssetDb`/texture cache at screen/load time (bounded). Fits our LRU/pinning; reuse existing texture reads, no duplicate loader. |
| **Font system: locale-aware font selection** | `core/fonts.lua:1-120` (centralized source of truth; WoW 3.3.5a locale mapping: enUS/deDE/frFR/esES/esMX/ptBR → `Fonts\FRIZQT__.TTF`+`Fonts\ARIALN.TTF`, ruRU Cyrillic FRIZQT, koKR `2002.TTF`, zhCN `ZYKai_T.TTF`, zhTW `bLEI00D.TTF`; custom `expressway.ttf`/`PTSansNarrow` cover Latin-1/EE/Cyrillic but no CJK; fallback selection per locale; exposed as `addon.Fonts`, loaded early via `DragonUI.xml`) | Single font resolver choosing custom vs system/CJK-safe font by locale with explicit coverage notes | **ADOPT (concept) / ADAPT (impl)** | **Adopt** the policy: one font-selection layer, locale/script-aware, prefer game/custom Latin fonts but fall back to CJK-capable font when locale/glyphs require; never hardcode paths per module. **Adapt** to our stack: drive from our locale/font assets (we already load `Fonts\FRIZQT__.TTF` from MPQ in wc3-game) and add a resolver API + fallback chain; integrate CJK fallback only when such assets/locales exist (no invented fonts). Pairs with our `load_game_font` path. |
| **Font assets** | `DragonUI/Fonts/expressway.ttf` (116.6K, only TTF in DragonUI; Options fonts empty) | Bundled Latin display font | **ADAPT** | Keep as donor reference for UI styling; do not ship/copy assets into repo blindly. If a UI face is needed, source/load via our asset pipeline with provenance. |
| **Scaling: UI scale with floor bypass + resync** | `core/uiscale.lua` (Blizzard slider floor 0.64; uses `UIParent:SetScale` directly; tracks `ownScale` vs `blizzardScale` so first enable starts from current screen scale; `Sync()` applies wanted scale and sends `DRAGONUI_UI_SCALE_CHANGED`; re-syncs a frame later on `PLAYER_LOGIN`, `UI_SCALE_CHANGED`, `DISPLAY_SIZE_CHANGED`, `UPDATE_FLOATING_CHAT_WINDOWS` because client resets; `SafeExecute("uiscale","sync",Sync)`) | Precise UI scaling independent of engine slider floor, with ownership tracking and event-driven reapplication | **ADOPT (pattern) / ADAPT (impl)** | **Adopt** pattern: separate "desired UI scale" from engine default, remember engine-owned vs app-owned scale, reapply on resize/display events, emit a scale-changed signal for layout recompute. **Adapt** to our renderer/HUD: apply to our UI root scale/DPI (not `UIParent`), clamp to safe range, trigger HUD relayout. High-value for mobile parity/resolution handling. |
| **Scaling/layout: per-frame scale + resize hooks** | `core/editor_settings.lua` sliders `unitframe.*.scale` 0.5–2.0 step 0.01 with `R.*` rebuild/`resize=true`; orientation/padding with rebuild | Per-module scale stored in profile and applied with layout rebuild | **ADAPT** | Port per-module scale factor concept into HUD module state (bounded 0.5–2.0) with relayout/rebuild on change; aligns with position presets. |
| **Data/reference tables** | `data/*.lua` (aura_durations, companion_traits, mount_speeds, npc_rare_ranks) | WoW-specific gameplay/UI lookup data | **REJECT** | WoW domain data, not WC3/herofolends; no value and would violate "original game data only (MPQ/SLK/FDF/W3E/MDX)". |
| **WoW frame/combat/module logic** | `modules/**` (unitframes, nameplates, actionbars, talents, spellbook, characterpanel, merchant, collections, professions, etc.), `core/{modelview,reload,visibility_fade,combatlog_fix}.lua`, libs (LibKeyBound/AceGUI) | WoW addon runtime against Blizzard API/events/widgets | **REJECT (runtime) / ADAPT (select UI layout ideas only)** | Do not port WoW API/event/widget code. Only mine layout/visual composition ideas where they map to our HUD; all behavior must be reimplemented on wc3-game/wc3-assetdb/wc3-audio + original WC3 data. |
| **Licensing/provenance** | `LICENSE` (MIT), `LICENSES/` dirs, headers "Portions adapted from RetailUI and DragonflightUI (both MIT); see THIRD_PARTY_NOTICES", `CONTRIBUTING.md` | Donor provenance recorded | **ADOPT (process)** | Preserve provenance/attribution notes in intake; port mechanisms/patterns into this private project with clean-room-style reimplementation where required; never copy donor code/assets blindly without tracking origin. |

## 3. Strongest mechanisms to port (priority, for this project)

1. **Scaling pattern (ADOPT/ADAPT, highest):** desired-vs-engine scale ownership + event re-sync + scale-changed signal (`uiscale.lua`) → deterministic HUD/UI scaling on mobile/resize.  
2. **Layout persistence (ADAPT):** quadrant-anchor positions with layout-offset compensation (`position_presets.lua`) → stable, resolution-aware HUD placement.  
3. **Font selection layer (ADOPT/ADAPT):** single locale/script-aware resolver with CJK fallback policy (`fonts.lua`) over MPQ/system fonts → correct glyph coverage without per-module hardcoding.  
4. **Atlas/texture warmup (ADAPT):** named sprite/atlas registry concept + bounded preload lists (`atlasinfo` usage, `texture_warmup.lua`) on top of `wc3-assetdb` cache → faster/consistent UI texture access.

## 4. Risks / non-goals

- Donor is WoW 3.3.5a Lua addon (Blizzard frame/event APIs, `UIParent`, Ace libs) — **not directly runnable** in Rust/wc3 stack; all ports are conceptual/mechanistic reimplementations.  
- Atlas data (`atlasinfo` tables) and module logic are WoW-specific; only patterns port, not data.  
- 418 BLP/243 TGA textures are donor art assets — reference only; our renderer must use original WC3 MPQ assets (per invariant).  
- No code merged into `wc3-game` UI/HUD by this intake; HUD-owned integration remains a follow-up (patch/coordination) task.

## 5. Evidence (commands)

```sh
find ~/DragonUI-20261113 -type f | wc -l            # 1107
du -sh ~/DragonUI-20261113                          # 254M
find . -type f -name "*.*" | sed 's/.*\.//' | sort | uniq -c | sort -rn
# core.xml, api.lua, uiscale.lua, position_presets.lua, editor_panel.lua,
# editor_settings.lua, commands.lua, fonts.lua, texture_warmup.lua inspected
grep -rn "atlasinfo" --include="*.lua" . | head     # 17+ modules
```

**Result:** Intake complete with per-mechanism adopt/adapt/reject verdicts (layout, atlases, font system, scaling). No donor code executed; read-only analysis.
